use sled::transaction::TransactionalTree;

/// A constructor for key generator contexts.
/// Separate from [`GenerateKey`], so that we can implement cases where we don't actually need to generate any keys (i.e. all are provided).
pub trait KeyGenerator<Context = DefaultContext> {
    fn construct(context: Context, tree: &TransactionalTree) -> Self;
}

/// Default key generation context, used when calling [`DBTree::insert`] method without specifying any context.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultContext;

pub trait GenerateKey<Key> {
    fn generate_next(&self) -> Key;
}

/// A "placeholder" key generator type that indicates no automatic key generation, i.e. key must always be explicitly provided.
pub struct KeyMustBeProvided;

impl KeyGenerator for KeyMustBeProvided {
    fn construct(_context: DefaultContext, _tree: &TransactionalTree) -> Self {
        KeyMustBeProvided
    }
}
