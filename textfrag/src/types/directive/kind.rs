#[derive(PartialEq, Clone, Debug, Default)]
pub enum TextDirectiveKind {
    /// Prefix
    Prefix,
    /// Start
    #[default]
    Start,
    /// End
    End,
    /// Suffix
    Suffix,
}
