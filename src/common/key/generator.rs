use sled::transaction::TransactionalTree;

/// A constructor for key generator contexts.
/// Separate from [`GenerateKey`], so that we can implement cases where we don't actually need to generate any keys (i.e. all are provided).
pub trait KeyGenerator<Context = DefaultContext> {
    fn construct(context: Context, tree: &TransactionalTree) -> Self;
}

/// Default key generation context, used when calling [`DBTree::insert`] method without specifying any context.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultContext;

impl<T: Default> KeyGenerator<DefaultContext> for T {
    fn construct(_: DefaultContext, _: &TransactionalTree) -> Self {
        Default::default()
    }
}

pub trait GenerateKey<Key> {
    fn generate_next(&self) -> Key;
}
