//! A tiny RGBA color type that reads `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.

use serde::{Deserialize, Deserializer};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Alpha as a float in `0.0..=1.0`.
    pub fn alpha(self) -> f32 {
        f32::from(self.a) / 255.0
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColorError(pub String);

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid color `{}` (expected #rgb, #rrggbb or #rrggbbaa)", self.0)
    }
}

impl std::error::Error for ParseColorError {}

impl FromStr for Rgba {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_owned());
        let hex = s.trim().strip_prefix('#').ok_or_else(err)?;
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(err());
        }
        let nibble = |i: usize| u8::from_str_radix(&hex[i..=i], 16).map(|v| v * 17);
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
        let c = match hex.len() {
            3 | 4 => Rgba {
                r: nibble(0).map_err(|_| err())?,
                g: nibble(1).map_err(|_| err())?,
                b: nibble(2).map_err(|_| err())?,
                a: if hex.len() == 4 { nibble(3).map_err(|_| err())? } else { 255 },
            },
            6 | 8 => Rgba {
                r: byte(0).map_err(|_| err())?,
                g: byte(2).map_err(|_| err())?,
                b: byte(4).map_err(|_| err())?,
                a: if hex.len() == 8 { byte(6).map_err(|_| err())? } else { 255 },
            },
            _ => return Err(err()),
        };
        Ok(c)
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            write!(f, "#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_forms() {
        assert_eq!("#fff".parse::<Rgba>().unwrap(), Rgba::rgb(255, 255, 255));
        assert_eq!("#0f08".parse::<Rgba>().unwrap(), Rgba { r: 0, g: 255, b: 0, a: 136 });
        assert_eq!("#e6a15c".parse::<Rgba>().unwrap(), Rgba::rgb(0xe6, 0xa1, 0x5c));
        assert_eq!("#e6a15c33".parse::<Rgba>().unwrap().a, 0x33);
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "fff", "#ff", "#gggggg", "#12345", "#ééé"] {
            assert!(bad.parse::<Rgba>().is_err(), "{bad}");
        }
    }

    #[test]
    fn roundtrips_display() {
        for s in ["#15141a", "#c0532f26"] {
            assert_eq!(s.parse::<Rgba>().unwrap().to_string(), s);
        }
    }
}
