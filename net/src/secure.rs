//! Encrypted, checked links to friends nearby (the local Wi-Fi or
//! Bluetooth).
//!
//! Over Tor the onion address already proves who answered and Tor
//! encrypts everything. A nearby link is a plain socket on a shared
//! network, so before anything else both phones run this handshake
//! (in the style of SIGMA-I / Noise XX, with signatures by the phones'
//! identity keys):
//!
//! 1. The dialer sends a fresh X25519 key and a short hint derived from
//!    the key of the phone it means to reach. A phone that isn't the one
//!    meant hangs up without saying who it is.
//! 2. The listener answers with its own fresh key and, encrypted under the
//!    two keys' shared secret, its identity key and a signature over the
//!    handshake so far.
//! 3. The dialer checks that this is the friend it meant to reach, then
//!    sends its own identity key and signature, encrypted the same way.
//!
//! Identity keys are never sent in the clear. From then on every record
//! is sealed with ChaCha20-Poly1305 under a key per direction, numbered so
//! nothing can be replayed, reordered or dropped unnoticed; anything that
//! fails to open ends the link. The usual hello and everything else then
//! runs inside.

use crate::engine::BoxConn;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sanctum_core::nearby::verify_handshake;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use x25519_dalek::{EphemeralSecret, PublicKey};

/// Starts the dialer's first message: "an encrypted nearby link, v1".
const MAGIC: [u8; 4] = *b"SNS1";
const HINT_LEN: usize = 16;
const MSG1_LEN: usize = 4 + 32 + HINT_LEN;
const TAG_LEN: usize = 16;
/// An identity key and a signature, sealed.
const SEALED_ID_LEN: usize = 32 + 64 + TAG_LEN;
const MSG2_LEN: usize = 32 + SEALED_ID_LEN;
/// Plaintext per record; the length before each record is two bytes.
pub const RECORD: usize = 16 * 1024;
/// Bytes buffered between the link and the code using it.
const PIPE: usize = 64 * 1024;

/// What this phone needs for the handshake: its key, and signing its part
/// (see `Session::sign_nearby_handshake`).
pub trait Signer: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    fn sign(&self, dialer: bool, transcript: &[u8; 32]) -> [u8; 64];
}

/// Tells the phone meant (`listener`) that it is the one being reached,
/// without telling anyone who doesn't already know its key.
fn hint(listener: &[u8; 32], ephemeral: &[u8; 32]) -> [u8; HINT_LEN] {
    let mut h = blake3::Hasher::new_derive_key("sanctum nearby hint v1");
    h.update(listener);
    h.update(ephemeral);
    h.finalize().as_bytes()[..HINT_LEN].try_into().unwrap()
}

fn chain(previous: &[u8], added: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new_derive_key("sanctum nearby transcript v1");
    h.update(previous);
    h.update(added);
    *h.finalize().as_bytes()
}

/// Two 32-byte keys from the shared secret and the transcript.
fn keys(context: &str, shared: &[u8; 32], transcript: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut h = blake3::Hasher::new_derive_key(context);
    h.update(shared);
    h.update(transcript);
    let mut out = [0u8; 64];
    h.finalize_xof().fill(&mut out);
    (out[..32].try_into().unwrap(), out[32..].try_into().unwrap())
}

fn nonce(counter: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&counter.to_be_bytes());
    n.into()
}

fn seal_id(key: &[u8; 32], transcript: &[u8; 32], id: &[u8; 32], sig: &[u8; 64]) -> Vec<u8> {
    let plain = [id.as_slice(), sig].concat();
    ChaCha20Poly1305::new(key.into())
        .encrypt(&nonce(0), Payload { msg: &plain, aad: transcript })
        .expect("sealing can't fail")
}

/// The identity and signature inside a sealed handshake message, if it
/// opens and the signature is good.
fn open_id(key: &[u8; 32], transcript: &[u8; 32], sealed: &[u8], dialer: bool, signed: &[u8; 32]) -> Option<[u8; 32]> {
    let plain = ChaCha20Poly1305::new(key.into()).decrypt(&nonce(0), Payload { msg: sealed, aad: transcript }).ok()?;
    let id: [u8; 32] = plain.get(..32)?.try_into().ok()?;
    let sig: [u8; 64] = plain.get(32..96)?.try_into().ok()?;
    verify_handshake(&id, dialer, signed, &sig).then_some(id)
}

fn shared(secret: EphemeralSecret, theirs: [u8; 32]) -> anyhow::Result<[u8; 32]> {
    let shared = secret.diffie_hellman(&PublicKey::from(theirs));
    anyhow::ensure!(shared.was_contributory(), "bad nearby handshake");
    Ok(*shared.as_bytes())
}

/// Per-direction traffic keys, once both sides have proved who they are.
struct Traffic {
    dialer_to_listener: [u8; 32],
    listener_to_dialer: [u8; 32],
}

impl Traffic {
    fn new(shared: &[u8; 32], transcript: &[u8; 32]) -> Self {
        let (dialer_to_listener, listener_to_dialer) = keys("sanctum nearby traffic v1", shared, transcript);
        Self { dialer_to_listener, listener_to_dialer }
    }
}

/// Reaches `peer` over `conn` (a nearby socket): encrypts the link and
/// makes sure `peer` is who answered. Returns the link to use instead.
pub async fn dial(mut conn: BoxConn, me: &dyn Signer, peer: [u8; 32]) -> anyhow::Result<BoxConn> {
    let secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let ours = PublicKey::from(&secret).to_bytes();
    let msg1 = [MAGIC.as_slice(), &ours, &hint(&peer, &ours)].concat();
    conn.write_all(&msg1).await?;
    conn.flush().await?;

    let mut msg2 = [0u8; MSG2_LEN];
    conn.read_exact(&mut msg2).await?;
    let theirs: [u8; 32] = msg2[..32].try_into().unwrap();
    let shared = shared(secret, theirs)?;
    let h1 = chain(&msg1, &theirs);
    let (listener_key, dialer_key) = keys("sanctum nearby handshake v1", &shared, &h1);
    let who = open_id(&listener_key, &h1, &msg2[32..], false, &h1);
    anyhow::ensure!(who == Some(peer), "someone else answered nearby");

    let h2 = chain(&h1, &msg2[32..]);
    let msg3 = seal_id(&dialer_key, &h2, &me.public_key(), &me.sign(true, &h2));
    conn.write_all(&msg3).await?;
    conn.flush().await?;
    let h3 = chain(&h2, &msg3);
    let traffic = Traffic::new(&shared, &h3);
    Ok(pipe(conn, traffic.dialer_to_listener, traffic.listener_to_dialer))
}

/// Answers a nearby connection: encrypts the link and learns who dialed,
/// proved by their signature. Hangs up on anyone not trying to reach this
/// phone, without saying who it is.
pub async fn accept(mut conn: BoxConn, me: &dyn Signer) -> anyhow::Result<(BoxConn, [u8; 32])> {
    let mut msg1 = [0u8; MSG1_LEN];
    conn.read_exact(&mut msg1).await?;
    anyhow::ensure!(msg1[..4] == MAGIC, "not a nearby handshake");
    let theirs: [u8; 32] = msg1[4..36].try_into().unwrap();
    anyhow::ensure!(msg1[36..] == hint(&me.public_key(), &theirs), "not for this phone");

    let secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let ours = PublicKey::from(&secret).to_bytes();
    let shared = shared(secret, theirs)?;
    let h1 = chain(&msg1, &ours);
    let (listener_key, dialer_key) = keys("sanctum nearby handshake v1", &shared, &h1);
    let sealed = seal_id(&listener_key, &h1, &me.public_key(), &me.sign(false, &h1));
    conn.write_all(&[ours.as_slice(), &sealed].concat()).await?;
    conn.flush().await?;

    let h2 = chain(&h1, &sealed);
    let mut msg3 = [0u8; SEALED_ID_LEN];
    conn.read_exact(&mut msg3).await?;
    let who = open_id(&dialer_key, &h2, &msg3, true, &h2).ok_or_else(|| anyhow::anyhow!("bad nearby handshake"))?;
    anyhow::ensure!(who != me.public_key(), "bad nearby handshake");
    let h3 = chain(&h2, &msg3);
    let traffic = Traffic::new(&shared, &h3);
    Ok((pipe(conn, traffic.listener_to_dialer, traffic.dialer_to_listener), who))
}

/// Runs the sealed link in the background; the caller reads and writes
/// plain bytes on the returned end. Anything that doesn't open (tampered,
/// replayed, reordered, cut short) closes both.
fn pipe(conn: BoxConn, send_key: [u8; 32], recv_key: [u8; 32]) -> BoxConn {
    let (near, far) = tokio::io::duplex(PIPE);
    let (mut plain_in, mut plain_out) = tokio::io::split(far);
    let (mut wire_in, mut wire_out) = tokio::io::split(conn);
    tokio::spawn(async move {
        let up = async {
            let cipher = ChaCha20Poly1305::new(&send_key.into());
            let mut chunk = vec![0u8; RECORD];
            let mut counter = 0u64;
            loop {
                let n = plain_in.read(&mut chunk).await?;
                if n == 0 {
                    return anyhow::Ok(());
                }
                let len = ((n + TAG_LEN) as u16).to_be_bytes();
                let sealed = cipher
                    .encrypt(&nonce(counter), Payload { msg: &chunk[..n], aad: &len })
                    .map_err(|_| anyhow::anyhow!("sealing failed"))?;
                counter = counter.checked_add(1).ok_or_else(|| anyhow::anyhow!("link too long"))?;
                wire_out.write_all(&[len.as_slice(), &sealed].concat()).await?;
                wire_out.flush().await?;
            }
        };
        let down = async {
            let cipher = ChaCha20Poly1305::new(&recv_key.into());
            let mut counter = 0u64;
            let mut record = vec![0u8; RECORD + TAG_LEN];
            loop {
                let mut len = [0u8; 2];
                wire_in.read_exact(&mut len).await?;
                let n = u16::from_be_bytes(len) as usize;
                anyhow::ensure!((TAG_LEN + 1..=RECORD + TAG_LEN).contains(&n), "bad record");
                wire_in.read_exact(&mut record[..n]).await?;
                let plain = cipher
                    .decrypt(&nonce(counter), Payload { msg: &record[..n], aad: &len })
                    .map_err(|_| anyhow::anyhow!("a record didn't open"))?;
                counter = counter.checked_add(1).ok_or_else(|| anyhow::anyhow!("link too long"))?;
                plain_out.write_all(&plain).await?;
            }
            #[allow(unreachable_code)]
            anyhow::Ok(())
        };
        // Either way ending ends the link: the socket and the plain end
        // are dropped together.
        tokio::select! {
            _ = up => {}
            _ = down => {}
        }
    });
    Box::new(near)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sanctum_core::identity::Identity;
    use sanctum_core::nearby::handshake_message;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::timeout;

    impl Signer for Identity {
        fn public_key(&self) -> [u8; 32] {
            Identity::public_key(self)
        }
        fn sign(&self, dialer: bool, transcript: &[u8; 32]) -> [u8; 64] {
            Identity::sign(self, &handshake_message(&Identity::public_key(self), dialer, transcript))
        }
    }

    async fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (a, b) = tokio::join!(TcpStream::connect(addr), listener.accept());
        (a.unwrap(), b.unwrap().0)
    }

    /// A socket pair over loopback TCP.
    async fn sockets() -> (BoxConn, BoxConn) {
        let (a, b) = tcp_pair().await;
        (Box::new(a), Box::new(b))
    }

    /// As [`sockets`], with everything that crosses recorded, and each
    /// direction's bytes (`true`: from the first end) passed through
    /// `meddle` once past the handshake. A direction ends when its sender
    /// closes, or after `meddle` shortens a chunk.
    async fn tapped(meddle: fn(bool, &mut Vec<u8>)) -> (BoxConn, BoxConn, Arc<Mutex<Vec<u8>>>) {
        let (a, relay_a) = tcp_pair().await;
        let (relay_b, b) = tcp_pair().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (ra, wa) = relay_a.into_split();
        let (rb, wb) = relay_b.into_split();
        for (dir, mut from, mut into) in [(true, ra, wb), (false, rb, wa)] {
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut sent = 0usize;
                let mut chunk = [0u8; 4096];
                loop {
                    let n = match from.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    let mut bytes = chunk[..n].to_vec();
                    seen.lock().unwrap().extend_from_slice(&bytes);
                    let mut cut = false;
                    if sent >= SEALED_ID_LEN {
                        meddle(dir, &mut bytes);
                        cut = bytes.len() < n;
                    }
                    sent += n;
                    if into.write_all(&bytes).await.is_err() || cut {
                        return;
                    }
                }
            });
        }
        (Box::new(a), Box::new(b), seen)
    }

    async fn pair(a: BoxConn, b: BoxConn, alice: &Identity, bob: &Identity) -> (BoxConn, BoxConn) {
        let (dialed, accepted) = tokio::join!(dial(a, alice, bob.public_key()), accept(b, bob));
        let (accepted, who) = accepted.unwrap();
        assert_eq!(who, alice.public_key(), "Bob learns who dialed");
        (dialed.unwrap(), accepted)
    }

    async fn read_some(conn: &mut BoxConn) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 64 * 1024];
        match timeout(Duration::from_secs(5), conn.read(&mut buf)).await.expect("hung") {
            Ok(0) | Err(_) => None,
            Ok(n) => Some(buf[..n].to_vec()),
        }
    }

    #[tokio::test]
    async fn both_sides_prove_who_they_are_and_talk_privately() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let (a, b, seen) = tapped(|_, _| {}).await;
        let (mut a, mut b) = pair(a, b, &alice, &bob).await;
        a.write_all(b"secret words from alice").await.unwrap();
        assert_eq!(read_some(&mut b).await.unwrap(), b"secret words from alice");
        b.write_all(b"and bob's").await.unwrap();
        assert_eq!(read_some(&mut a).await.unwrap(), b"and bob's");
        // Bigger than a record, both ways at once.
        let big: Vec<u8> = (0..100_000u32).map(|i| i as u8).collect();
        let (big_a, big_b) = (big.clone(), big.clone());
        let send = tokio::spawn(async move {
            a.write_all(&big_a).await.unwrap();
            a
        });
        let mut got = Vec::new();
        while got.len() < big.len() {
            got.extend(read_some(&mut b).await.unwrap());
        }
        assert_eq!(got, big_b);
        let _a = send.await.unwrap();

        // Someone watching the network sees neither key nor the words.
        let seen = seen.lock().unwrap();
        for secret in [alice.public_key().as_slice(), bob.public_key().as_slice(), b"secret words", &big[..64]] {
            assert!(!seen.windows(secret.len()).any(|w| w == secret), "leaked in the clear");
        }
    }

    #[tokio::test]
    async fn the_dialer_refuses_anyone_but_the_friend_it_meant() {
        let (alice, bob, eve) = (Identity::generate(), Identity::generate(), Identity::generate());
        // Eve answers a dial meant for Bob: she isn't the one meant, so
        // she can't even get the hint right, and hangs up.
        let (a, b) = sockets().await;
        let (dialed, accepted) = tokio::join!(dial(a, &alice, bob.public_key()), accept(b, &eve));
        assert!(dialed.is_err() && accepted.is_err());

        // An impostor who knows Bob's public key and answers as herself,
        // or as Bob with her own signature: refused, and Alice never says
        // who she is.
        for claim_bob in [false, true] {
            let (a, mut b) = sockets().await;
            let eve = Identity::from_secret(eve.secret());
            let bob_pk = bob.public_key();
            let impostor = tokio::spawn(async move {
                let mut msg1 = [0u8; MSG1_LEN];
                b.read_exact(&mut msg1).await.unwrap();
                let secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
                let ours = PublicKey::from(&secret).to_bytes();
                let shared = shared(secret, msg1[4..36].try_into().unwrap()).unwrap();
                let h1 = chain(&msg1, &ours);
                let (listener_key, _) = keys("sanctum nearby handshake v1", &shared, &h1);
                let id = if claim_bob { bob_pk } else { eve.public_key() };
                let sig = Identity::sign(&eve, &handshake_message(&id, false, &h1));
                let sealed = seal_id(&listener_key, &h1, &id, &sig);
                b.write_all(&[ours.as_slice(), &sealed].concat()).await.unwrap();
                let mut rest = Vec::new();
                let _ = b.read_to_end(&mut rest).await;
                rest
            });
            assert!(dial(a, &alice, bob.public_key()).await.is_err());
            assert!(impostor.await.unwrap().is_empty(), "Alice sent nothing more");
        }
    }

    #[tokio::test]
    async fn tampered_replayed_or_cut_records_end_the_link() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        // A flipped bit.
        let (a, b, _) = tapped(|dir, bytes| {
            if dir {
                bytes[0] ^= 1;
            }
        })
        .await;
        let (mut a, mut b) = pair(a, b, &alice, &bob).await;
        a.write_all(&[7u8; 300]).await.unwrap();
        assert_eq!(read_some(&mut b).await, None, "closed, nothing delivered");

        // A record sent twice.
        let (a, b, _) = tapped(|dir, bytes| {
            if dir {
                let copy = bytes.clone();
                bytes.extend_from_slice(&copy);
            }
        })
        .await;
        let (mut a, mut b) = pair(a, b, &alice, &bob).await;
        a.write_all(&[7u8; 300]).await.unwrap();
        assert_eq!(read_some(&mut b).await.unwrap(), vec![7u8; 300], "the first copy");
        assert_eq!(read_some(&mut b).await, None, "the replay ends it");

        // A record cut short, then the connection closed: the half record
        // is never handed on.
        let (a, b, _) = tapped(|dir, bytes| {
            if dir {
                bytes.truncate(bytes.len() / 2);
            }
        })
        .await;
        let (mut a, mut b) = pair(a, b, &alice, &bob).await;
        a.write_all(&[7u8; 300]).await.unwrap();
        assert_eq!(read_some(&mut b).await, None, "nothing delivered");
    }

    #[tokio::test]
    async fn junk_never_panics_and_reveals_nothing() {
        let bob = Identity::generate();
        let mut inputs: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"SNS1".to_vec(),
            vec![0u8; MSG1_LEN],
            [MAGIC.as_slice(), &[0u8; 32], &hint(&bob.public_key(), &[0u8; 32])].concat(),
            vec![0xFF; 4096],
        ];
        // A right first message, then junk where the dialer's proof goes.
        let ours = PublicKey::from(&EphemeralSecret::random_from_rng(rand::rngs::OsRng)).to_bytes();
        inputs.push([MAGIC.as_slice(), &ours, &hint(&bob.public_key(), &ours), &[9u8; SEALED_ID_LEN]].concat());
        for input in inputs {
            let (mut a, b) = sockets().await;
            a.write_all(&input).await.unwrap();
            a.shutdown().await.unwrap();
            let result = timeout(Duration::from_secs(5), accept(b, &bob)).await.expect("hung");
            assert!(result.is_err());
            let mut back = Vec::new();
            let _ = a.read_to_end(&mut back).await;
            assert!(!back.windows(32).any(|w| w == bob.public_key()), "Bob's key never sent in the clear");
        }
        // The dialer, fed junk.
        for input in [Vec::new(), vec![0u8; MSG2_LEN], vec![0x41; MSG2_LEN + 10]] {
            let (a, mut b) = sockets().await;
            b.write_all(&input).await.unwrap();
            b.shutdown().await.unwrap();
            let result = timeout(Duration::from_secs(5), dial(a, &bob, Identity::generate().public_key())).await;
            assert!(result.expect("hung").is_err());
        }
    }
}
