macro_rules! wrap_variants {
    // This arm is used to avoid nested loops with the arguments
    // The arguments are transformed to $combined_args TokenTree
    (@match $self:ident $fnc:ident $combined_args:tt [$($variant:ident),+]) => {
        match $self {
            $(
                Self::$variant(inner) => inner.$fnc$combined_args,
            )+
        }
    };

    ($fnc:ident, $ret:ty, [$($arg:ident: $t:ty),*], [$($variant:ident),+]) => {
        fn $fnc(&self, $($arg: $t),*) -> $ret {
            wrap_variants!(@match self $fnc ($($arg,)*) [$($variant),+])
        }
    };

    ($variants:tt $(fnc!($fnc:ident, $ret:ty, $args:tt);)+) => {
        $(
            wrap_variants!($fnc, $ret, $args, $variants);
        )*
    };
}

pub(crate) use wrap_variants;

