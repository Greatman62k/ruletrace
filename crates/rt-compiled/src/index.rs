//! Index newtypes. A compiled name is an index into one table, and each table has its own type,
//! so an input index cannot be used as a rule index.

macro_rules! index_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Wraps an index. Whether it is in range is checked when the Compiled_Rulebook
            /// is built.
            #[must_use]
            pub fn new(value: u32) -> Self {
                Self(value)
            }

            /// The index of position `value` in a table, if it fits in 32 bits.
            #[must_use]
            pub fn from_usize(value: usize) -> Option<Self> {
                u32::try_from(value).ok().map(Self)
            }

            /// The index as stored in the artifact.
            #[must_use]
            pub fn get(self) -> u32 {
                self.0
            }

            /// The index as a position in a table.
            #[must_use]
            pub fn at(self) -> usize {
                usize::try_from(self.0).unwrap_or(usize::MAX)
            }
        }
    };
}

index_type!(
    /// Position in `inputs`.
    InputIdx
);
index_type!(
    /// Position in `collections`.
    CollectionIdx
);
index_type!(
    /// Position in the `fields` of one collection.
    FieldIdx
);
index_type!(
    /// Position in `enums`.
    EnumIdx
);
index_type!(
    /// Position in the `members` of one enum.
    MemberIdx
);
index_type!(
    /// Position in `sets`.
    SetIdx
);
index_type!(
    /// Position in `rules` (ascending rule identifier).
    RuleIdx
);
index_type!(
    /// Position in [`CompiledRulebook::targets`](crate::CompiledRulebook::targets): the
    /// distinct Target names of the rules in ascending byte order.
    TargetIdx
);
index_type!(
    /// Position in the node arena of one rule.
    NodeIdx
);
