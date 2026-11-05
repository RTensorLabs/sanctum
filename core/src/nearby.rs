//! Recognizing friends' phones over Bluetooth without telling strangers
//! anything.
//!
//! A Bluetooth advert holds only a few bytes, so a phone can't carry a tag
//! per friend (as the Wi-Fi beacon does). Instead each phone advertises
//! one short ID that changes every minute, computed from a secret "nearby
//! key". The key is shared with friends only, inside MLS
//! (`Content::NearbyKey`); a friend computes our ID for this minute and
//! recognizes it. To anyone else the ID is random, and it can't be linked
//! from one minute to the next.
//!
//! The Wi-Fi beacon's per-friend tags are keyed with both friends' nearby
//! keys once they have swapped them ([`pair_key`]), so knowing two phones'
//! public keys isn't enough to follow the pair around.
//!
//! Links to nearby friends are encrypted and both phones prove who they
//! are before anything else is said (see `sanctum_net::secure`); this
//! module signs this phone's part of that handshake
//! ([`Session::sign_nearby_handshake`]). Phones that do that advertise
//! IDs of a new kind (`v2`), so they never try to reach older phones,
//! which speak in the clear, over Bluetooth; those still meet over Tor.

use crate::identity::verify;
use crate::protocol::Content;
use crate::session::{Result, Session};
use crate::store::{OutboxItem, OutboxState};

/// Bytes of the rotating ID in an advert.
pub const ID_LEN: usize = 8;

/// This phone's nearby key, derived from its identity secret: stable, and
/// never sent to anyone but friends.
pub fn nearby_key(identity_secret: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key("sanctum nearby key v1", identity_secret)
}

/// The ID a phone with `key` advertises during `minute` (Unix minutes).
pub fn nearby_id(key: &[u8; 32], minute: u64) -> [u8; ID_LEN] {
    // v2: phones that encrypt nearby links (older ones advertise v1).
    let mut input = b"sanctum nearby id v2".to_vec();
    input.extend_from_slice(&minute.to_le_bytes());
    blake3::keyed_hash(key, &input).as_bytes()[..ID_LEN].try_into().unwrap()
}

/// The secret a pair of friends share once both have the other's nearby
/// key: the same on both phones, unknown to anyone else.
pub fn pair_key(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = blake3::Hasher::new_derive_key("sanctum nearby pair key v1");
    hasher.update(lo);
    hasher.update(hi);
    *hasher.finalize().as_bytes()
}

/// What a phone signs in the nearby handshake: a label only this
/// handshake uses, which side it is, its own key and the handshake so far.
pub fn handshake_message(signer: &[u8; 32], dialer: bool, transcript: &[u8; 32]) -> Vec<u8> {
    [b"sanctum nearby handshake v1".as_slice(), &[u8::from(dialer)], signer, transcript].concat()
}

/// Whether `signature` is `signer`'s for this handshake.
pub fn verify_handshake(signer: &[u8; 32], dialer: bool, transcript: &[u8; 32], signature: &[u8; 64]) -> bool {
    verify(signer, &handshake_message(signer, dialer, transcript), signature)
}

/// A friend, and the secret we share for nearby tags if we have one.
pub type FriendPairKey = ([u8; 32], Option<[u8; 32]>);

fn kv_name(peer: &[u8; 32]) -> Vec<u8> {
    [b"nearby-key:".as_slice(), peer].concat()
}

impl Session {
    /// What this phone advertises right now.
    pub fn my_nearby_id(&self, minute: u64) -> [u8; ID_LEN] {
        nearby_id(&nearby_key(&self.identity.secret()), minute)
    }

    /// Signs this phone's part of a nearby handshake (`dialer`: we
    /// connected). Signs nothing else.
    pub fn sign_nearby_handshake(&self, dialer: bool, transcript: &[u8; 32]) -> [u8; 64] {
        self.identity.sign(&handshake_message(&self.public_key(), dialer, transcript))
    }

    /// Each friend, with the secret the two of us share for nearby tags
    /// once we have each other's nearby key (None until then).
    pub fn nearby_pair_keys(&self) -> Result<Vec<FriendPairKey>> {
        let mine = nearby_key(&self.identity.secret());
        self.friend_keys()?
            .into_iter()
            .map(|friend| {
                let theirs = self.store.kv_get::<[u8; 32]>(&kv_name(&friend))?;
                Ok((friend, theirs.map(|k| pair_key(&mine, &k))))
            })
            .collect()
    }

    /// The friend whose phone advertises `id` (this minute or the last:
    /// clocks differ a little).
    pub fn nearby_friend(&self, id: &[u8], minute: u64) -> Result<Option<[u8; 32]>> {
        for friend in self.friend_keys()? {
            let Some(key) = self.store.kv_get::<[u8; 32]>(&kv_name(&friend))? else {
                continue;
            };
            if [minute, minute.saturating_sub(1)].iter().any(|m| nearby_id(&key, *m) == id) {
                return Ok(Some(friend));
            }
        }
        Ok(None)
    }

    /// Before flushing to a friend: if we don't know their nearby key yet,
    /// send ours (they answer with theirs). Once per friend per run.
    pub(crate) fn offer_nearby_key(&mut self, peer: &[u8; 32]) -> Result<()> {
        if self.store.kv_get::<[u8; 32]>(&kv_name(peer))?.is_some() || !self.nearby_offered.insert(*peer) {
            return Ok(());
        }
        self.queue_nearby_key(peer)
    }

    /// A friend's nearby key arrived.
    pub(crate) fn on_nearby_key(&mut self, peer: &[u8; 32], key: [u8; 32]) -> Result<()> {
        let known = self.store.kv_get::<[u8; 32]>(&kv_name(peer))?;
        self.store.kv_put(&kv_name(peer), &key)?;
        // New to us: they may not have ours either.
        if known.is_none() && self.nearby_offered.insert(*peer) {
            self.queue_nearby_key(peer)?;
        }
        Ok(())
    }

    pub(crate) fn forget_nearby_key(&self, peer: &[u8; 32]) -> Result<()> {
        self.store.kv_delete(&kv_name(peer))?;
        Ok(())
    }

    fn queue_nearby_key(&mut self, peer: &[u8; 32]) -> Result<()> {
        let content = Content::NearbyKey(nearby_key(&self.identity.secret()));
        self.store.outbox_put(
            &rand::random(),
            &OutboxItem {
                peer: *peer,
                state: OutboxState::Plain(content.encode()),
                message: None,
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    #[test]
    fn handshake_signatures_are_bound_to_side_signer_and_transcript() {
        let (a, b) = (Identity::generate(), Identity::generate());
        let t = [5u8; 32];
        let sig = a.sign(&handshake_message(&a.public_key(), true, &t));
        assert!(verify_handshake(&a.public_key(), true, &t, &sig));
        assert!(!verify_handshake(&a.public_key(), false, &t, &sig), "the other side's");
        assert!(!verify_handshake(&a.public_key(), true, &[6; 32], &sig), "another handshake");
        assert!(!verify_handshake(&b.public_key(), true, &t, &sig), "someone else");
        // Never mistaken for a hello.
        let hello = crate::protocol::hello_message(&b.public_key(), &t);
        assert!(!verify(&a.public_key(), &hello, &sig));
    }

    #[test]
    fn friends_share_a_pair_key_once_connected() {
        use crate::session::tests::{befriend, connect, Phone};
        let (mut alice, mut bob) = (Phone::new(), Phone::new());
        befriend(&mut alice, &mut bob);
        assert_eq!(alice.s.nearby_pair_keys().unwrap(), [(bob.pk(), None)], "not before they meet");
        connect(&mut alice, &mut bob);
        connect(&mut bob, &mut alice);
        let (a, b) = (alice.s.nearby_pair_keys().unwrap(), bob.s.nearby_pair_keys().unwrap());
        assert_eq!(a[0].0, bob.pk());
        assert!(a[0].1.is_some() && a[0].1 == b[0].1, "{a:?} {b:?}");
        assert_ne!(a[0].1, Some(crate::identity::pair_fingerprint(&alice.pk(), &bob.pk())));
    }

    #[test]
    fn pair_keys_match_on_both_phones_only() {
        let (ka, kb, kc) = ([1u8; 32], [2u8; 32], [3u8; 32]);
        assert_eq!(pair_key(&ka, &kb), pair_key(&kb, &ka));
        assert_ne!(pair_key(&ka, &kb), pair_key(&ka, &kc));
    }
}
