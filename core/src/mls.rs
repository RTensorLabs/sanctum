//! End-to-end encryption for every chat (a 1:1 chat is a two-member group),
//! using MLS (RFC 9420) via OpenMLS.
//!
//! - The MLS signing key *is* the Sanctum identity key, and a member's MLS
//!   identity must equal their signing key, so nobody can claim to be
//!   someone else inside a group.
//! - Messages get forward secrecy and post-compromise security from MLS's
//!   ratchets; application messages are padded to hide their length.
//! - All MLS state lives in memory while running and is persisted, sealed,
//!   in the encrypted [`Store`] via [`MlsClient::persist`].

use crate::identity::Identity;
use crate::store::{Store, StoreError};
use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{MemoryStorage, RustCrypto};
use openmls_traits::types::SignatureScheme;
use openmls_traits::OpenMlsProvider;
use std::collections::HashMap;
use std::fmt::Debug;

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

/// Messages are padded to a multiple of this many bytes.
const PADDING: usize = 64;
/// Groups have no server to order messages: one sent just before a
/// membership change can arrive just after it. Keys for this many past
/// epochs are kept so it can still be read.
const PAST_EPOCHS: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("MLS: {0}")]
    Protocol(String),
    #[error("no such group")]
    NoSuchGroup,
    #[error("a member's identity does not match their key")]
    ImpostorCredential,
    #[error("not a member of this group")]
    NotAMember,
    /// A membership change from someone who may not make one (only the
    /// group's leader may). Nothing was changed: not even a key was used up, so the same message can
    /// still be taken later if its sender turns out to be allowed.
    #[error("membership change refused")]
    Refused,
    #[error("storage: {0}")]
    Store(#[from] StoreError),
}

fn protocol<E: Debug>(e: E) -> MlsError {
    MlsError::Protocol(format!("{e:?}"))
}

pub type Result<T> = std::result::Result<T, MlsError>;

#[derive(Default)]
struct Provider {
    crypto: RustCrypto,
    storage: MemoryStorage,
}

impl OpenMlsProvider for Provider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &MemoryStorage {
        &self.storage
    }

    fn crypto(&self) -> &RustCrypto {
        &self.crypto
    }

    fn rand(&self) -> &RustCrypto {
        &self.crypto
    }
}

/// What a received message turned out to be.
#[derive(Debug, PartialEq)]
pub enum Received {
    Message {
        sender: [u8; 32],
        plaintext: Vec<u8>,
    },
    /// Someone joined or left; refresh the member list.
    MembersChanged,
    /// We were removed from the group.
    Removed,
}

/// A welcome for a new member plus the commit existing members must apply.
pub struct Invite {
    pub commit: Vec<u8>,
    pub welcome: Vec<u8>,
}

pub struct MlsClient {
    provider: Provider,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    /// What the store holds, so [`persist`](Self::persist) writes only changes.
    persisted: HashMap<Vec<u8>, Vec<u8>>,
}

impl MlsClient {
    /// Restores the client from `store` (empty on first run).
    pub fn load(identity: &Identity, store: &Store) -> Result<Self> {
        let saved = store.mls_load()?;
        let provider = Provider::default();
        *provider.storage.values.write().unwrap() = saved.clone();
        let public = identity.public_key();
        let signer = SignatureKeyPair::from_raw(
            SignatureScheme::ED25519,
            identity.secret().to_vec(),
            public.to_vec(),
        );
        let credential = CredentialWithKey {
            credential: BasicCredential::new(public.to_vec()).into(),
            signature_key: public.to_vec().into(),
        };
        Ok(Self {
            provider,
            signer,
            credential,
            persisted: saved,
        })
    }

    /// Writes any state changed since the last call into the encrypted store.
    /// Call after every operation; until then, changes exist only in memory.
    pub fn persist(&mut self, store: &Store) -> Result<()> {
        self.persist_with::<()>(store, None)
    }

    /// Like [`persist`](Self::persist), saving `put` in the store's small
    /// values in the same step (see [`Store::mls_apply_with`]).
    pub fn persist_with<T: serde::Serialize>(&mut self, store: &Store, put: Option<(&[u8], &T)>) -> Result<()> {
        // Only what changed is copied: a big group's state is large, and
        // this runs after every message.
        let (upserts, deletes) = {
            let current = self.provider.storage.values.read().unwrap();
            let upserts: Vec<(Vec<u8>, Vec<u8>)> = current
                .iter()
                .filter(|(k, v)| self.persisted.get(*k) != Some(v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let deletes: Vec<Vec<u8>> =
                self.persisted.keys().filter(|k| !current.contains_key(*k)).cloned().collect();
            (upserts, deletes)
        };
        if upserts.is_empty() && deletes.is_empty() && put.is_none() {
            return Ok(());
        }
        let up: Vec<(&[u8], &[u8])> = upserts.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
        let del: Vec<&[u8]> = deletes.iter().map(Vec::as_slice).collect();
        store.mls_apply_with(&up, &del, put)?;
        for k in &deletes {
            self.persisted.remove(k);
        }
        self.persisted.extend(upserts);
        Ok(())
    }

    /// A single-use key package others need to add us to a group. Shared in
    /// person through the invite QR.
    pub fn key_package(&self) -> Result<Vec<u8>> {
        let bundle = KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .map_err(protocol)?;
        bundle
            .key_package()
            .tls_serialize_detached()
            .map_err(protocol)
    }

    /// Starts a new group with just us in it. Returns its id.
    pub fn create_group(&self) -> Result<Vec<u8>> {
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .padding_size(PADDING)
            .max_past_epochs(PAST_EPOCHS)
            .build();
        let group = MlsGroup::new(
            &self.provider,
            &self.signer,
            &config,
            self.credential.clone(),
        )
        .map_err(protocol)?;
        Ok(group.group_id().as_slice().to_vec())
    }

    /// Adds the owner of `key_package` to the group.
    pub fn invite(&self, group_id: &[u8], key_package: &[u8]) -> Result<Invite> {
        let kp = KeyPackageIn::tls_deserialize_exact(key_package)
            .map_err(protocol)?
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(protocol)?;
        let leaf = kp.leaf_node();
        check_identity(leaf.credential(), leaf.signature_key().as_slice())?;

        let mut group = self.group(group_id)?;
        // Our change carries only what we chose (see `receive`).
        group.clear_pending_proposals(self.provider.storage()).map_err(protocol)?;
        let (commit, welcome, _) = group
            .add_members(&self.provider, &self.signer, &[kp])
            .map_err(protocol)?;
        group
            .merge_pending_commit(&self.provider)
            .map_err(protocol)?;
        Ok(Invite {
            commit: commit.to_bytes().map_err(protocol)?,
            welcome: welcome.to_bytes().map_err(protocol)?,
        })
    }

    /// Like [`invite`](Self::invite), but only if `key_package` is
    /// `expected`'s: checked before anything changes.
    pub fn invite_expecting(&self, group_id: &[u8], key_package: &[u8], expected: &[u8; 32]) -> Result<Invite> {
        let kp = KeyPackageIn::tls_deserialize_exact(key_package)
            .map_err(protocol)?
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(protocol)?;
        if kp.leaf_node().signature_key().as_slice() != expected {
            return Err(MlsError::Protocol("key package from someone else".into()));
        }
        self.invite(group_id, key_package)
    }

    /// Joins a group from a welcome. Returns the group id.
    pub fn join(&self, welcome: &[u8]) -> Result<Vec<u8>> {
        let MlsMessageBodyIn::Welcome(welcome) = MlsMessageIn::tls_deserialize_exact(welcome)
            .map_err(protocol)?
            .extract()
        else {
            return Err(MlsError::Protocol("not a welcome".into()));
        };
        let config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .padding_size(PADDING)
            .max_past_epochs(PAST_EPOCHS)
            .build();
        let group = StagedWelcome::new_from_welcome(&self.provider, &config, welcome, None)
            .map_err(protocol)?
            .into_group(&self.provider)
            .map_err(protocol)?;
        check_members(&group)?;
        Ok(group.group_id().as_slice().to_vec())
    }

    pub fn encrypt(&self, group_id: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut group = self.group(group_id)?;
        group
            .create_message(&self.provider, &self.signer, plaintext)
            .map_err(protocol)?
            .to_bytes()
            .map_err(protocol)
    }

    /// Decrypts an application message or applies a membership change.
    ///
    /// Only `committer` may change who is in the group: a change from
    /// anyone else (or with no `committer`, from anyone at all, as in a
    /// friends' pair) is refused before it's applied, and so is every
    /// proposal (none of our phones send them; stored, the next change we
    /// make would carry them along). A refused message leaves the group's
    /// state exactly as it was.
    pub fn receive(&self, group_id: &[u8], bytes: &[u8], committer: Option<&[u8; 32]>) -> Result<Received> {
        let mut group = self.group(group_id)?;
        let message = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(protocol)?
            .try_into_protocol_message()
            .map_err(protocol)?;
        // Reading a membership change uses up a key even if we then refuse
        // it, so the state from before is kept to put back. (Changes are
        // rare; messages, the common case, don't need this.)
        let before = message
            .is_handshake_message()
            .then(|| self.provider.storage.values.read().unwrap().clone());
        let restore = |error: MlsError| {
            if let Some(before) = &before {
                *self.provider.storage.values.write().unwrap() = before.clone();
            }
            Err(error)
        };
        let processed = group
            .process_message(&self.provider, message)
            .map_err(protocol)?;
        let sender_credential = processed.credential().clone();
        let from_member = matches!(processed.sender(), Sender::Member(_));
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                let sender = identity_of(&sender_credential)?;
                Ok(Received::Message {
                    sender,
                    plaintext: app.into_bytes(),
                })
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                let by = identity_of(&sender_credential).ok();
                if !from_member || by.is_none() || by.as_ref() != committer {
                    return restore(MlsError::Refused);
                }
                group
                    .merge_staged_commit(&self.provider, *commit)
                    .map_err(protocol)?;
                if !group.is_active() {
                    return Ok(Received::Removed);
                }
                check_members(&group)?;
                Ok(Received::MembersChanged)
            }
            // Never sent by our phones: refused for good.
            ProcessedMessageContent::ProposalMessage(_) => {
                restore(MlsError::Protocol("proposals are not accepted".into()))
            }
            other => Err(MlsError::Protocol(format!("unexpected message: {other:?}"))),
        }
    }

    /// Removes a member. Returns the commit for the remaining members.
    pub fn remove(&self, group_id: &[u8], member: &[u8; 32]) -> Result<Vec<u8>> {
        let mut group = self.group(group_id)?;
        let index = group
            .members()
            .find(|m| m.signature_key == member)
            .ok_or(MlsError::NotAMember)?
            .index;
        group.clear_pending_proposals(self.provider.storage()).map_err(protocol)?;
        let (commit, _, _) = group
            .remove_members(&self.provider, &self.signer, &[index])
            .map_err(protocol)?;
        group
            .merge_pending_commit(&self.provider)
            .map_err(protocol)?;
        commit.to_bytes().map_err(protocol)
    }

    /// The group's current epoch (it moves on with every membership change).
    pub fn epoch(&self, group_id: &[u8]) -> Result<u64> {
        Ok(self.group(group_id)?.epoch().as_u64())
    }

    /// Members' public keys.
    pub fn members(&self, group_id: &[u8]) -> Result<Vec<[u8; 32]>> {
        let group = self.group(group_id)?;
        group
            .members()
            .map(|m| identity_of(&m.credential))
            .collect()
    }

    fn group(&self, group_id: &[u8]) -> Result<MlsGroup> {
        MlsGroup::load(self.provider.storage(), &GroupId::from_slice(group_id))
            .map_err(protocol)?
            .ok_or(MlsError::NoSuchGroup)
    }
}

/// The epoch a group message was sent in (it travels in the clear in the
/// message's header), or `None` if it isn't one.
pub fn message_epoch(bytes: &[u8]) -> Option<u64> {
    let message = MlsMessageIn::tls_deserialize_exact(bytes).ok()?.try_into_protocol_message().ok()?;
    Some(message.epoch().as_u64())
}

/// How many past epochs' keys a member keeps (see [`PAST_EPOCHS`]).
pub const KEPT_EPOCHS: u64 = PAST_EPOCHS as u64;

fn identity_of(credential: &Credential) -> Result<[u8; 32]> {
    BasicCredential::try_from(credential.clone())
        .map_err(protocol)?
        .identity()
        .try_into()
        .map_err(|_| MlsError::ImpostorCredential)
}

/// A credential's claimed identity must be the key that signs for it.
fn check_identity(credential: &Credential, signature_key: &[u8]) -> Result<()> {
    if identity_of(credential)?.as_slice() == signature_key {
        Ok(())
    } else {
        Err(MlsError::ImpostorCredential)
    }
}

fn check_members(group: &MlsGroup) -> Result<()> {
    group
        .members()
        .try_for_each(|m| check_identity(&m.credential, &m.signature_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A device: identity, its own encrypted store, and an MLS client.
    struct Device {
        identity: Identity,
        store: Store,
        mls: MlsClient,
        _dir: TempDir,
    }

    impl Device {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let identity = Identity::generate();
            let store = Store::open(dir.path().join("s.db"), &rand::random()).unwrap();
            let mls = MlsClient::load(&identity, &store).unwrap();
            Self {
                identity,
                store,
                mls,
                _dir: dir,
            }
        }

        fn pk(&self) -> [u8; 32] {
            self.identity.public_key()
        }

        fn save(&mut self) {
            self.mls.persist(&self.store).unwrap();
        }

        /// Simulates an app restart: drop the in-memory client and rebuild
        /// it purely from the encrypted store.
        fn restart(&mut self) {
            self.save();
            self.mls = MlsClient::load(&self.identity, &self.store).unwrap();
        }

        fn send(&mut self, group: &[u8], text: &str) -> Vec<u8> {
            let ct = self.mls.encrypt(group, text.as_bytes()).unwrap();
            self.save();
            ct
        }

        fn recv(&mut self, group: &[u8], ct: &[u8]) -> Received {
            self.recv_led(group, ct, None)
        }

        /// Receives in a group led by `leader` (who alone may change it).
        fn recv_led(&mut self, group: &[u8], ct: &[u8], leader: Option<&Device>) -> Received {
            let r = self.mls.receive(group, ct, leader.map(|l| l.identity.public_key()).as_ref()).unwrap();
            self.save();
            r
        }
    }

    fn msg(sender: &Device, text: &str) -> Received {
        Received::Message {
            sender: sender.pk(),
            plaintext: text.as_bytes().to_vec(),
        }
    }

    /// Alice creates a group and adds Bob; both persist.
    fn pair() -> (Device, Device, Vec<u8>) {
        let (mut alice, mut bob) = (Device::new(), Device::new());
        let kp = bob.mls.key_package().unwrap();
        bob.save();
        let group = alice.mls.create_group().unwrap();
        let invite = alice.mls.invite(&group, &kp).unwrap();
        alice.save();
        assert_eq!(bob.mls.join(&invite.welcome).unwrap(), group);
        bob.save();
        (alice, bob, group)
    }

    #[test]
    fn two_people_chat_both_ways() {
        let (mut alice, mut bob, group) = pair();
        let ct = alice.send(&group, "hi bob");
        assert_eq!(bob.recv(&group, &ct), msg(&alice, "hi bob"));
        let ct = bob.send(&group, "hi alice");
        assert_eq!(alice.recv(&group, &ct), msg(&bob, "hi alice"));
        let mut members = alice.mls.members(&group).unwrap();
        members.sort();
        let mut expected = vec![alice.pk(), bob.pk()];
        expected.sort();
        assert_eq!(members, expected);
    }

    #[test]
    fn sessions_survive_restarts_from_encrypted_store() {
        let (mut alice, mut bob, group) = pair();
        alice.restart();
        bob.restart();
        let ct = alice.send(&group, "after restart");
        bob.restart();
        assert_eq!(bob.recv(&group, &ct), msg(&alice, "after restart"));
        // Keep ratcheting across several restarts in both directions.
        for i in 0..5 {
            let ct = bob.send(&group, &format!("ping {i}"));
            alice.restart();
            assert_eq!(alice.recv(&group, &ct), msg(&bob, &format!("ping {i}")));
            let ct = alice.send(&group, &format!("pong {i}"));
            bob.restart();
            assert_eq!(bob.recv(&group, &ct), msg(&alice, &format!("pong {i}")));
        }
    }

    #[test]
    fn unsaved_state_is_not_on_disk() {
        let (alice, mut bob, group) = pair();
        // Bob receives but "crashes" before persisting: on reload he still
        // has the pre-message state and can process the message again.
        let ct = alice.mls.encrypt(&group, b"x").unwrap();
        bob.mls.receive(&group, &ct, None).unwrap();
        bob.mls = MlsClient::load(&bob.identity, &bob.store).unwrap();
        assert_eq!(bob.recv(&group, &ct), msg(&alice, "x"));
    }

    #[test]
    fn group_of_three_add_and_remove() {
        let (mut alice, mut bob, group) = pair();
        let mut carol = Device::new();
        let kp = carol.mls.key_package().unwrap();
        carol.save();
        let invite = alice.mls.invite(&group, &kp).unwrap();
        alice.save();
        assert_eq!(bob.recv_led(&group, &invite.commit, Some(&alice)), Received::MembersChanged);
        carol.mls.join(&invite.welcome).unwrap();
        carol.restart();

        let ct = carol.send(&group, "hello both");
        assert_eq!(alice.recv(&group, &ct), msg(&carol, "hello both"));
        assert_eq!(bob.recv(&group, &ct), msg(&carol, "hello both"));

        // Carol is removed and can no longer read the group.
        let commit = alice.mls.remove(&group, &carol.pk()).unwrap();
        alice.save();
        assert_eq!(bob.recv_led(&group, &commit, Some(&alice)), Received::MembersChanged);
        assert_eq!(carol.recv_led(&group, &commit, Some(&alice)), Received::Removed);
        let ct = alice.send(&group, "carol is gone");
        assert_eq!(bob.recv(&group, &ct), msg(&alice, "carol is gone"));
        assert!(carol.mls.receive(&group, &ct, None).is_err());
        assert_eq!(alice.mls.members(&group).unwrap().len(), 2);
    }

    /// Alice leads a group with Bob and Carol in it.
    fn trio() -> (Device, Device, Device, Vec<u8>) {
        let (mut alice, mut bob, group) = pair();
        let mut carol = Device::new();
        let kp = carol.mls.key_package().unwrap();
        carol.save();
        let invite = alice.mls.invite(&group, &kp).unwrap();
        alice.save();
        assert_eq!(bob.recv_led(&group, &invite.commit, Some(&alice)), Received::MembersChanged);
        carol.mls.join(&invite.welcome).unwrap();
        carol.save();
        (alice, bob, carol, group)
    }

    fn sorted(mut v: Vec<[u8; 32]>) -> Vec<[u8; 32]> {
        v.sort();
        v
    }

    #[test]
    fn only_the_leader_changes_who_is_in_a_group() {
        let (mut alice, mut bob, mut carol, group) = trio();
        let members = sorted(alice.mls.members(&group).unwrap());
        let epoch = carol.mls.epoch(&group).unwrap();

        // Bob (a member, not the leader) removes Alice and adds Eve.
        let removal = bob.mls.remove(&group, &alice.pk()).unwrap();
        let mut eve = Device::new();
        let kp = eve.mls.key_package().unwrap();
        eve.save();
        assert!(matches!(carol.mls.receive(&group, &removal, Some(&alice.pk())), Err(MlsError::Refused)));
        // Without a leader (a friends' pair), nobody may change anything.
        assert!(matches!(carol.mls.receive(&group, &removal, None), Err(MlsError::Refused)));
        assert!(matches!(alice.mls.receive(&group, &removal, Some(&alice.pk())), Err(MlsError::Refused)));
        for who in [&mut carol, &mut alice] {
            who.restart();
            assert_eq!(sorted(who.mls.members(&group).unwrap()), members);
            assert_eq!(who.mls.epoch(&group).unwrap(), epoch);
        }
        // An outsider added by Bob is refused too.
        bob.mls = MlsClient::load(&bob.identity, &bob.store).unwrap();
        let add = bob.mls.invite(&group, &kp).unwrap();
        assert!(matches!(carol.mls.receive(&group, &add.commit, Some(&alice.pk())), Err(MlsError::Refused)));
        assert!(eve.mls.join(&add.welcome).is_ok(), "Eve's own copy is all she gets");

        // The leader's changes still count, and the group still works.
        let ct = alice.send(&group, "still here");
        assert_eq!(carol.recv(&group, &ct), msg(&alice, "still here"));
        let mut dave = Device::new();
        let kp = dave.mls.key_package().unwrap();
        dave.save();
        let invite = alice.mls.invite(&group, &kp).unwrap();
        alice.save();
        assert_eq!(carol.recv_led(&group, &invite.commit, Some(&alice)), Received::MembersChanged);
        assert_eq!(carol.mls.members(&group).unwrap().len(), 4);
    }

    #[test]
    fn a_refused_change_can_still_be_taken_once_its_sender_may_make_it() {
        // A new leader's change can arrive before word that they lead.
        let (mut alice, mut bob, mut carol, group) = trio();
        let commit = bob.mls.remove(&group, &carol.pk()).unwrap();
        bob.save();
        assert!(matches!(alice.mls.receive(&group, &commit, Some(&alice.pk())), Err(MlsError::Refused)));
        alice.restart();
        assert_eq!(alice.recv_led(&group, &commit, Some(&bob)), Received::MembersChanged);
        assert_eq!(alice.mls.members(&group).unwrap().len(), 2);
        let ct = bob.send(&group, "just us");
        assert_eq!(alice.recv(&group, &ct), msg(&bob, "just us"));
        let _ = &mut carol;
    }

    #[test]
    fn proposals_are_refused_and_never_ride_along() {
        let (mut alice, mut bob, carol, group) = trio();
        // Bob proposes removing Carol, hoping Alice's next change carries it.
        let carol_index = {
            let g = bob.mls.group(&group).unwrap();
            let index = g.members().find(|m| m.signature_key == carol.pk()).unwrap().index;
            index
        };
        let proposal = {
            let mut g = bob.mls.group(&group).unwrap();
            let (out, _) = g.propose_remove_member(&bob.mls.provider, &bob.mls.signer, carol_index).unwrap();
            out.to_bytes().unwrap()
        };
        bob.save();
        assert!(matches!(alice.mls.receive(&group, &proposal, Some(&alice.pk())), Err(MlsError::Protocol(_))));
        alice.save();
        // An older build kept such proposals: Alice's phone did, then updated.
        {
            let mut g = alice.mls.group(&group).unwrap();
            let message = MlsMessageIn::tls_deserialize_exact(&proposal).unwrap().try_into_protocol_message().unwrap();
            let ProcessedMessageContent::ProposalMessage(p) =
                g.process_message(&alice.mls.provider, message).unwrap().into_content()
            else {
                panic!("a proposal");
            };
            g.store_pending_proposal(alice.mls.provider.storage(), *p).unwrap();
            assert!(g.has_pending_proposals());
        }
        alice.restart();
        // Alice adds Dave: Carol stays.
        let mut dave = Device::new();
        let kp = dave.mls.key_package().unwrap();
        dave.save();
        alice.mls.invite(&group, &kp).unwrap();
        let members = alice.mls.members(&group).unwrap();
        assert!(members.contains(&carol.pk()) && members.contains(&dave.pk()));
    }

    #[test]
    fn ciphertext_hides_content_and_length() {
        let (alice, _bob, group) = pair();
        let short = alice.mls.encrypt(&group, b"hi").unwrap();
        let longer = alice
            .mls
            .encrypt(&group, b"the meeting is at the old bridge")
            .unwrap();
        assert!(!short.windows(2).any(|w| w == b"hi"));
        assert!(!longer.windows(6).any(|w| w == b"bridge"));
        assert_eq!(short.len(), longer.len(), "padding hides message length");
    }

    #[test]
    fn tampered_and_replayed_messages_are_rejected() {
        let (alice, mut bob, group) = pair();
        let ct = alice.mls.encrypt(&group, b"original").unwrap();
        let mut bad = ct.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(bob.mls.receive(&group, &bad, None).is_err());
        assert_eq!(bob.recv(&group, &ct), msg(&alice, "original"));
        assert!(bob.mls.receive(&group, &ct, None).is_err(), "replay must fail");
    }

    #[test]
    fn outsiders_cannot_read() {
        let (alice, _bob, group) = pair();
        let mut eve = Device::new();
        let eve_group = eve.mls.create_group().unwrap();
        let ct = alice.mls.encrypt(&group, b"secret").unwrap();
        assert!(eve.mls.receive(&group, &ct, None).is_err());
        assert!(eve.mls.receive(&eve_group, &ct, None).is_err());
        eve.save();
    }

    #[test]
    fn impostor_key_package_is_refused() {
        // Mallory claims Bob's identity but signs with her own key.
        let bob = Device::new();
        let mallory_id = Identity::generate();
        let provider = Provider::default();
        let signer = SignatureKeyPair::from_raw(
            SignatureScheme::ED25519,
            mallory_id.secret().to_vec(),
            mallory_id.public_key().to_vec(),
        );
        let forged = KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &provider,
                &signer,
                CredentialWithKey {
                    credential: BasicCredential::new(bob.pk().to_vec()).into(),
                    signature_key: mallory_id.public_key().to_vec().into(),
                },
            )
            .unwrap()
            .key_package()
            .tls_serialize_detached()
            .unwrap();
        let alice = Device::new();
        let group = alice.mls.create_group().unwrap();
        assert!(matches!(
            alice.mls.invite(&group, &forged),
            Err(MlsError::ImpostorCredential)
        ));
    }

    #[test]
    fn mls_state_on_disk_is_sealed() {
        let (mut alice, _bob, group) = pair();
        alice.save();
        let path = alice._dir.path().join("s.db");
        let mut bytes = std::fs::read(&path).unwrap();
        if let Ok(wal) = std::fs::read(alice._dir.path().join("s.db-wal")) {
            bytes.extend(wal);
        }
        assert!(
            !bytes.windows(group.len()).any(|w| w == group.as_slice()),
            "group id leaked"
        );
        assert!(
            !bytes.windows(32).any(|w| w == alice.identity.secret()),
            "secret key leaked"
        );
    }
}
