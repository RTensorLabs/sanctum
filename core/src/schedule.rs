//! Messages written now and sent later, by this phone.
//!
//! They wait here, sealed in the store, until their time; then they go
//! like any other message (the phone has to be on, with Sanctum running in
//! the background, at that time; otherwise they go as soon as it is).

use crate::session::{clip, now, Event, Result, Session, SessionError};
use crate::store::StoreError;
use serde::{Deserialize, Serialize};

const SCHEDULED: &[u8] = b"scheduled";
/// More than this waiting at once is refused.
const MAX_SCHEDULED: usize = 100;
/// The furthest ahead a message can be scheduled, in seconds.
const MAX_AHEAD: u64 = 366 * 24 * 3600;
/// A message that couldn't be sent at its time is tried again after this
/// many seconds...
const RETRY_AFTER: u64 = 60;
/// ...this many times while the app runs; then it waits for the next start.
const TRIES: u8 = 5;

/// A message waiting for its time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scheduled {
    pub id: u64,
    pub chat_id: i64,
    pub body: String,
    /// When it goes, in Unix seconds.
    pub at: u64,
}

impl Session {
    fn scheduled_all(&self) -> Result<Vec<Scheduled>> {
        Ok(self.store.kv_get(SCHEDULED)?.unwrap_or_default())
    }

    fn save_scheduled(&mut self, all: &[Scheduled]) -> Result<()> {
        self.store.kv_put(SCHEDULED, &all)?;
        self.scheduled_next = all.iter().map(|s| s.at).min().unwrap_or(u64::MAX);
        Ok(())
    }

    /// Sends `body` in the chat at `at` (Unix seconds). Returns its id.
    pub fn schedule_message(&mut self, chat_id: i64, body: &str, at: u64) -> Result<u64> {
        self.chat(chat_id)?;
        let body = clip(body.trim(), 4000);
        if body.is_empty() {
            return Err(SessionError::GroupRule("A message can't be empty."));
        }
        if at <= now() || at > now() + MAX_AHEAD {
            return Err(SessionError::GroupRule("Pick a time later today or in the coming year."));
        }
        let mut all = self.scheduled_all()?;
        if all.len() >= MAX_SCHEDULED {
            return Err(SessionError::GroupRule("Too many messages are waiting to be sent."));
        }
        let id = rand::random::<u64>() >> 11; // fits a double exactly
        all.push(Scheduled { id, chat_id, body, at });
        all.sort_by_key(|s| s.at);
        self.save_scheduled(&all)?;
        self.events.push(Event::Chat(chat_id));
        Ok(id)
    }

    /// Messages waiting in a chat, soonest first.
    pub fn scheduled(&self, chat_id: i64) -> Result<Vec<Scheduled>> {
        Ok(self.scheduled_all()?.into_iter().filter(|s| s.chat_id == chat_id).collect())
    }

    /// Forgets a waiting message.
    pub fn cancel_scheduled(&mut self, id: u64) -> Result<()> {
        let mut all = self.scheduled_all()?;
        if let Some(i) = all.iter().position(|s| s.id == id) {
            let gone = all.remove(i);
            self.save_scheduled(&all)?;
            self.events.push(Event::Chat(gone.chat_id));
        }
        Ok(())
    }

    /// Forgets everything waiting in a chat that's being deleted.
    pub(crate) fn forget_scheduled(&mut self, chat_id: i64) -> Result<()> {
        let mut all = self.scheduled_all()?;
        let before = all.len();
        all.retain(|s| s.chat_id != chat_id);
        if all.len() != before {
            self.save_scheduled(&all)?;
        }
        Ok(())
    }

    /// Sends the messages whose time has come. Cheap until one is due.
    /// Each is forgotten once sent, or if its chat is gone (deleted, or a
    /// group we're no longer in); one that fails otherwise stays to be
    /// tried again a little later.
    pub(crate) fn send_scheduled(&mut self) -> Result<()> {
        let now = now();
        if self.scheduled_next != 0 && self.scheduled_next > now {
            return Ok(());
        }
        let all = self.scheduled_all()?;
        if !all.iter().any(|s| s.at <= now) {
            self.scheduled_next = all.iter().map(|s| s.at).min().unwrap_or(u64::MAX);
            return Ok(());
        }
        let mut rest = Vec::new();
        for s in all {
            if s.at > now || self.scheduled_tries.get(&s.id).is_some_and(|t| *t >= TRIES) {
                rest.push(s);
                continue;
            }
            #[cfg(test)]
            if self.fail_sending > 0 {
                self.fail_sending -= 1;
                *self.scheduled_tries.entry(s.id).or_insert(0) += 1;
                rest.push(s);
                continue;
            }
            match self.send_to_chat(s.chat_id, &s.body, None) {
                Ok(reach) => {
                    self.scheduled_tries.remove(&s.id);
                    self.pokes.extend(reach);
                }
                Err(SessionError::NoSuchChat | SessionError::Store(StoreError::NoSuchChat) | SessionError::NotAFriend) => {
                    self.scheduled_tries.remove(&s.id);
                }
                Err(_) => {
                    *self.scheduled_tries.entry(s.id).or_insert(0) += 1;
                    rest.push(s);
                }
            }
        }
        self.save_scheduled(&rest)?;
        // One that failed goes again a little later, not straight away: a
        // failure that lasts mustn't keep the phone busy.
        self.scheduled_next = rest
            .iter()
            .filter(|s| self.scheduled_tries.get(&s.id).is_none_or(|t| *t < TRIES))
            .map(|s| if s.at <= now { now + RETRY_AFTER } else { s.at })
            .min()
            .unwrap_or(u64::MAX);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::{befriend, connect, Phone};

    #[test]
    fn scheduled_messages_go_at_their_time() {
        let (mut alice, mut bob) = (Phone::new(), Phone::new());
        let (ab, ba) = befriend(&mut alice, &mut bob);
        let soon = alice.s.schedule_message(ab, "happy birthday!", now() + 2).unwrap();
        let later = alice.s.schedule_message(ab, "never mind", now() + 3600).unwrap();
        assert!(alice.s.schedule_message(ab, "too late", now() - 1).is_err());
        assert_eq!(alice.s.scheduled(ab).unwrap().len(), 2);
        alice.s.purge();
        assert!(alice.texts(ab).is_empty(), "not yet");
        alice.s.cancel_scheduled(later).unwrap();
        // Its time comes (as if the clock moved on).
        let mut all = alice.s.scheduled_all().unwrap();
        all[0].at = now();
        assert_eq!(all[0].id, soon);
        alice.s.save_scheduled(&all).unwrap();
        alice.restart();
        alice.s.purge();
        assert!(alice.s.scheduled(ab).unwrap().is_empty());
        connect(&mut alice, &mut bob);
        assert_eq!(bob.texts(ba), [(false, "happy birthday!".into(), true)]);
    }

    /// Makes every waiting message due now (as if the clock moved on).
    fn all_due(phone: &mut Phone) {
        let mut all = phone.s.scheduled_all().unwrap();
        for s in &mut all {
            s.at = now();
        }
        phone.s.save_scheduled(&all).unwrap();
    }

    #[test]
    fn a_scheduled_message_that_fails_waits_and_goes_later() {
        let (mut alice, mut bob) = (Phone::new(), Phone::new());
        let (ab, ba) = befriend(&mut alice, &mut bob);
        alice.s.schedule_message(ab, "on time", now() + 60).unwrap();
        all_due(&mut alice);
        alice.s.fail_sending = 1;
        alice.s.purge();
        assert_eq!(alice.s.scheduled(ab).unwrap().len(), 1, "kept after failing");
        assert!(alice.texts(ab).is_empty());
        // Not tried again straight away...
        alice.s.purge();
        assert!(alice.s.scheduled_next >= now() + RETRY_AFTER - 1);
        // ...but a little later.
        alice.s.scheduled_next = 0;
        alice.s.purge();
        assert!(alice.s.scheduled(ab).unwrap().is_empty());
        connect(&mut alice, &mut bob);
        assert_eq!(bob.texts(ba), [(false, "on time".into(), true)]);
    }

    #[test]
    fn a_scheduled_message_that_keeps_failing_doesnt_keep_the_phone_busy() {
        let (mut alice, mut bob) = (Phone::new(), Phone::new());
        let (ab, _) = befriend(&mut alice, &mut bob);
        alice.s.schedule_message(ab, "stuck", now() + 60).unwrap();
        all_due(&mut alice);
        alice.s.fail_sending = u8::MAX;
        for _ in 0..20 {
            alice.s.scheduled_next = 0;
            alice.s.purge();
        }
        assert_eq!(u8::MAX - alice.s.fail_sending, TRIES, "tried a few times only");
        assert_eq!(alice.s.scheduled(ab).unwrap().len(), 1, "still waiting, for the next start");
        // After a restart it's tried again.
        alice.restart();
        alice.s.purge();
        assert!(alice.s.scheduled(ab).unwrap().is_empty());
        assert_eq!(alice.texts(ab), [(true, "stuck".into(), false)]);
    }

    #[test]
    fn scheduled_messages_for_a_deleted_chat_are_dropped() {
        let (mut alice, mut bob) = (Phone::new(), Phone::new());
        let (ab, _) = befriend(&mut alice, &mut bob);
        alice.s.schedule_message(ab, "never", now() + 60).unwrap();
        alice.s.delete_chat(ab).unwrap();
        assert!(alice.s.scheduled_all().unwrap().is_empty(), "gone with the chat");
        // One left from before this was done goes when it's due.
        let gone = Scheduled { id: 7, chat_id: ab, body: "never".into(), at: now() };
        alice.s.save_scheduled(&[gone]).unwrap();
        alice.s.purge();
        assert!(alice.s.scheduled_all().unwrap().is_empty());
    }
}
