
// TODO(theomonnom): Match the complete function signature like:
//  - pub(crate) fn update_info(&self, info: ParticipantInfo) -> ();
#[macro_export]
macro_rules! enum_dispatch {
    // This arm is used to avoid nested loops with the arguments
    // The arguments are transformed to $combined_args TokenTree
    (@match $self:ident $fnc:ident $combined_args:tt [$($variant:ident),+]) => {
        match $self {
            $(
                Self::$variant(inner) => inner.$fnc$combined_args,
            )+
        }
    };

    ($vis:vis$(,)? $fnc:ident, $self:ty, [$($arg:ident: $t:ty),*], $ret:ty, [$($variant:ident),+]) => {
        $vis fn $fnc(self: $self, $($arg: $t),*) -> $ret {
            enum_dispatch!(@match self $fnc ($($arg,)*) [$($variant),+])
        }
    };

    ($variants:tt $(fnc!($vis:vis$(,)? $fnc:ident, $self:ty, $args:tt, $ret:ty);)+) => {
        $(
            enum_dispatch!($vis, $fnc, $self, $args, $ret, $variants);
        )*
    };
}
