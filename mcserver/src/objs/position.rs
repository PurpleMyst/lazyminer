use serde::{
    de::{self, Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};

#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Debug)]
pub struct Position {
    pub x: i32, // 26 bits
    pub y: i16, // 12 bits
    pub z: i32, // 26 bits
}

impl Into<u64> for Position {
    fn into(self) -> u64 {
        let x: u64 = self.x as u64 & lsb!(26);
        let y: u64 = self.y as u64 & lsb!(12);
        let z: u64 = self.z as u64 & lsb!(26);

        (x << (26 + 12)) | (y << 26) | z
    }
}

impl From<u64> for Position {
    fn from(n: u64) -> Self {
        let z = (n & lsb!(26)) as u32;
        let y = ((n >> 26) & lsb!(12)) as u16;
        let x = ((n >> (26 + 12)) & lsb!(26)) as u32;

        macro_rules! uN_to_iN {
            ($x:ident: $N:literal; $from_ty:ty => $to_ty:ty) => {
                $x as $to_ty
                    - if $x >= (2 as $from_ty).pow($N - 1) {
                        (2 as $to_ty).pow($N)
                    } else {
                        0
                    }
            };
        };

        let x = uN_to_iN!(x: 26; u32 => i32);
        let y = uN_to_iN!(y: 12; u16 => i16);
        let z = uN_to_iN!(z: 26; u32 => i32);

        Self { x, y, z }
    }
}

impl Serialize for Position {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.clone().into())
    }
}

impl<'de> Deserialize<'de> for Position {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PositionVisitor;

        impl Visitor<'_> for PositionVisitor {
            type Value = Position;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a 64 bit integer representing a Position")
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Position::from(value))
            }
        }

        deserializer.deserialize_u64(PositionVisitor)
    }
}
