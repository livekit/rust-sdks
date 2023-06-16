use std::fmt;

macro_rules! id_str {
    ($($name:ident;)*) => {
        $(
            impl From<String> for $name {
                fn from(str: String) -> $name {
                    $name(str)
                }
            }

            impl From<$name> for String {
                fn from(id: $name) -> String {
                    id.0
                }
            }

            impl PartialEq<$name> for String {
                fn eq(&self, u: &$name) -> bool {
                    *self == *u.0
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str(&self.0)
                }
            }
        )*
    }
}

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ParticipantSid(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ParticipantIdentity(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TrackSid(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct RoomSid(pub String);

id_str! {
    ParticipantSid;
    ParticipantIdentity;
    TrackSid;
    RoomSid;
}
