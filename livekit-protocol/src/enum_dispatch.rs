// TODO(theomonnom): Async methods
#[macro_export]
macro_rules! enum_dispatch {
    // This arm is used to avoid nested loops with the arguments
    // The arguments are transformed to $combined_args tt
    (@match [$($variant:ident),+]: $fnc:ident, $self:ident, $combined_args:tt) => {
        match $self {
            $(
                Self::$variant(inner) => inner.$fnc$combined_args,
            )+
        }
    };

    // Create the function and extract self fron the $args tt (little hack)
    (@fnc [$($variant:ident),+]: $vis:vis fn $fnc:ident($self:ident: $sty:ty $(, $arg:ident: $t:ty)*) -> $ret:ty) => {
        #[inline]
        $vis fn $fnc($self: $sty, $($arg: $t),*) -> $ret {
            enum_dispatch!(@match [$($variant),+]: $fnc, $self, ($($arg,)*))
        }
    };

    ($variants:tt; $($vis:vis fn $fnc:ident$args:tt -> $ret:ty;)+) => {
        $(
            enum_dispatch!(@fnc $variants: $vis fn $fnc$args -> $ret);
        )+
    };
}
