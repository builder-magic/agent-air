// Turn identification for conversation tracking

use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};

/// Owner constants for turn identification
pub const OWNER_USER: &str = "u";
pub const OWNER_ASSISTANT: &str = "a";

/// Identifies a turn in the conversation (e.g., "u1", "a1", "u2", "a2")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TurnId {
    /// Owner of the turn: "u" for user, "a" for assistant
    pub owner: String,
    /// Turn number
    pub number: i64,
}

impl TurnId {
    /// Creates a new user turn with the given number
    pub fn new_user_turn(n: i64) -> Self {
        Self {
            owner: OWNER_USER.to_string(),
            number: n,
        }
    }

    /// Creates a new assistant turn with the given number
    pub fn new_assistant_turn(n: i64) -> Self {
        Self {
            owner: OWNER_ASSISTANT.to_string(),
            number: n,
        }
    }

    /// Parses a turn ID from a string (e.g., "u1", "a5")
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.len() < 2 {
            return Err(format!("invalid turn id: {}", s));
        }

        let owner = &s[0..1];
        if owner != OWNER_USER && owner != OWNER_ASSISTANT {
            return Err(format!("invalid turn owner: {}", owner));
        }

        let number: i64 = s[1..]
            .parse()
            .map_err(|_| format!("invalid turn number: {}", &s[1..]))?;

        Ok(Self {
            owner: owner.to_string(),
            number,
        })
    }

    /// Returns the next user turn (increments number)
    pub fn next_user_turn(&self) -> Self {
        Self {
            owner: OWNER_USER.to_string(),
            number: self.number + 1,
        }
    }

    /// Returns the next assistant turn (same number as current)
    pub fn next_assistant_turn(&self) -> Self {
        Self {
            owner: OWNER_ASSISTANT.to_string(),
            number: self.number,
        }
    }

    /// Returns true if this is a user turn
    pub fn is_user_turn(&self) -> bool {
        self.owner == OWNER_USER
    }

    /// Returns true if this is an assistant turn
    pub fn is_assistant_turn(&self) -> bool {
        self.owner == OWNER_ASSISTANT
    }
}

impl fmt::Display for TurnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.owner, self.number)
    }
}

/// Thread-safe counter for generating turn numbers
pub struct TurnCounter {
    counter: AtomicI64,
}

impl TurnCounter {
    /// Creates a new turn counter starting at 0
    pub fn new() -> Self {
        Self {
            counter: AtomicI64::new(0),
        }
    }

    /// Atomically increments and returns the next turn number
    pub fn next_number(&self) -> i64 {
        self.counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Returns the current turn number without incrementing
    pub fn current(&self) -> i64 {
        self.counter.load(Ordering::SeqCst)
    }
}

impl Default for TurnCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_id_creation() {
        let user_turn = TurnId::new_user_turn(1);
        assert_eq!(user_turn.owner, "u");
        assert_eq!(user_turn.number, 1);
        assert!(user_turn.is_user_turn());
        assert!(!user_turn.is_assistant_turn());

        let assistant_turn = TurnId::new_assistant_turn(1);
        assert_eq!(assistant_turn.owner, "a");
        assert_eq!(assistant_turn.number, 1);
        assert!(!assistant_turn.is_user_turn());
        assert!(assistant_turn.is_assistant_turn());
    }

    #[test]
    fn test_turn_id_parse() {
        let turn = TurnId::parse("u1").unwrap();
        assert_eq!(turn.owner, "u");
        assert_eq!(turn.number, 1);

        let turn = TurnId::parse("a42").unwrap();
        assert_eq!(turn.owner, "a");
        assert_eq!(turn.number, 42);

        assert!(TurnId::parse("x1").is_err());
        assert!(TurnId::parse("u").is_err());
        assert!(TurnId::parse("").is_err());
    }

    #[test]
    fn test_turn_id_display() {
        let turn = TurnId::new_user_turn(5);
        assert_eq!(turn.to_string(), "u5");

        let turn = TurnId::new_assistant_turn(3);
        assert_eq!(turn.to_string(), "a3");
    }

    #[test]
    fn test_turn_counter() {
        let counter = TurnCounter::new();
        assert_eq!(counter.current(), 0);
        assert_eq!(counter.next_number(), 1);
        assert_eq!(counter.next_number(), 2);
        assert_eq!(counter.current(), 2);
    }
}
