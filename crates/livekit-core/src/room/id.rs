macro_rules! id_str {
    ($($name:ident;)*) => {
        $(
            impl From<String> for $name {
                fn from(str: String) -> $name {
                    $name(str)
                }
            }

            impl PartialEq<$name> for String {
                fn eq(&self, u: &$name) -> bool {
                    *self == *u.0
                }
            }

            impl From<$name> for String {
                fn from(id: $name) -> String {
                    id.0
                }
            }
        )*
    }
}

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ParticipantSid(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ParticipantIdentity(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct TrackSid(pub String);

id_str! {
    ParticipantSid;
    ParticipantIdentity;
    TrackSid;
}
