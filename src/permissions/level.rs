//! Permission levels for the grant system.
//!
//! Permission levels form a hierarchy where higher levels imply lower levels:
//! `Admin > Execute > Write > Read > None`

use serde::{Deserialize, Serialize};
use std::fmt;

/// Permission level that determines what operations are allowed.
///
/// Levels are hierarchical: a higher level automatically grants all lower levels.
/// For example, `Write` permission implies `Read` permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PermissionLevel {
    /// No permission granted
    None = 0,
    /// Read-only access: view contents, list directories, search
    Read = 1,
    /// Read + create/modify: includes all Read operations plus file creation and modification
    Write = 2,
    /// Read + Write + execute: includes running commands and scripts
    Execute = 3,
    /// Full access: includes all operations plus dangerous ones (delete, system commands)
    Admin = 4,
}

impl PermissionLevel {
    /// Returns the numeric value of this permission level.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Creates a permission level from a numeric value.
    ///
    /// Returns `None` if the value is out of range (> 4).
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Read),
            2 => Some(Self::Write),
            3 => Some(Self::Execute),
            4 => Some(Self::Admin),
            _ => None,
        }
    }

    /// Checks if this permission level satisfies a required level.
    ///
    /// A level satisfies another if it is greater than or equal to the required level.
    /// This implements the permission hierarchy where higher levels imply lower levels.
    ///
    /// # Examples
    ///
    /// ```
    /// use agent_core::permissions::PermissionLevel;
    ///
    /// assert!(PermissionLevel::Write.satisfies(PermissionLevel::Read));
    /// assert!(PermissionLevel::Admin.satisfies(PermissionLevel::Execute));
    /// assert!(!PermissionLevel::Read.satisfies(PermissionLevel::Write));
    /// ```
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }

    /// Returns all permission levels that this level implies.
    ///
    /// For example, `Write` implies both `Write` and `Read`.
    pub fn implied_levels(self) -> Vec<Self> {
        match self {
            Self::None => vec![Self::None],
            Self::Read => vec![Self::None, Self::Read],
            Self::Write => vec![Self::None, Self::Read, Self::Write],
            Self::Execute => vec![Self::None, Self::Read, Self::Write, Self::Execute],
            Self::Admin => vec![
                Self::None,
                Self::Read,
                Self::Write,
                Self::Execute,
                Self::Admin,
            ],
        }
    }

    /// Returns all permission levels as a slice.
    pub fn all() -> &'static [Self] {
        &[
            Self::None,
            Self::Read,
            Self::Write,
            Self::Execute,
            Self::Admin,
        ]
    }
}

impl fmt::Display for PermissionLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Read => write!(f, "Read"),
            Self::Write => write!(f, "Write"),
            Self::Execute => write!(f, "Execute"),
            Self::Admin => write!(f, "Admin"),
        }
    }
}

impl Default for PermissionLevel {
    fn default() -> Self {
        Self::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_ordering() {
        assert!(PermissionLevel::Admin > PermissionLevel::Execute);
        assert!(PermissionLevel::Execute > PermissionLevel::Write);
        assert!(PermissionLevel::Write > PermissionLevel::Read);
        assert!(PermissionLevel::Read > PermissionLevel::None);
    }

    #[test]
    fn test_satisfies() {
        // Same level satisfies itself
        assert!(PermissionLevel::Read.satisfies(PermissionLevel::Read));
        assert!(PermissionLevel::Write.satisfies(PermissionLevel::Write));

        // Higher level satisfies lower
        assert!(PermissionLevel::Write.satisfies(PermissionLevel::Read));
        assert!(PermissionLevel::Execute.satisfies(PermissionLevel::Write));
        assert!(PermissionLevel::Admin.satisfies(PermissionLevel::Execute));

        // Admin satisfies all
        assert!(PermissionLevel::Admin.satisfies(PermissionLevel::None));
        assert!(PermissionLevel::Admin.satisfies(PermissionLevel::Read));
        assert!(PermissionLevel::Admin.satisfies(PermissionLevel::Write));
        assert!(PermissionLevel::Admin.satisfies(PermissionLevel::Execute));
        assert!(PermissionLevel::Admin.satisfies(PermissionLevel::Admin));

        // Lower level does not satisfy higher
        assert!(!PermissionLevel::Read.satisfies(PermissionLevel::Write));
        assert!(!PermissionLevel::Write.satisfies(PermissionLevel::Execute));
        assert!(!PermissionLevel::Execute.satisfies(PermissionLevel::Admin));
        assert!(!PermissionLevel::None.satisfies(PermissionLevel::Read));
    }

    #[test]
    fn test_from_u8() {
        assert_eq!(PermissionLevel::from_u8(0), Some(PermissionLevel::None));
        assert_eq!(PermissionLevel::from_u8(1), Some(PermissionLevel::Read));
        assert_eq!(PermissionLevel::from_u8(2), Some(PermissionLevel::Write));
        assert_eq!(PermissionLevel::from_u8(3), Some(PermissionLevel::Execute));
        assert_eq!(PermissionLevel::from_u8(4), Some(PermissionLevel::Admin));
        assert_eq!(PermissionLevel::from_u8(5), None);
    }

    #[test]
    fn test_implied_levels() {
        assert_eq!(
            PermissionLevel::Write.implied_levels(),
            vec![
                PermissionLevel::None,
                PermissionLevel::Read,
                PermissionLevel::Write
            ]
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", PermissionLevel::Read), "Read");
        assert_eq!(format!("{}", PermissionLevel::Admin), "Admin");
    }

    #[test]
    fn test_serialization() {
        let level = PermissionLevel::Write;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"Write\"");

        let deserialized: PermissionLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, level);
    }
}
