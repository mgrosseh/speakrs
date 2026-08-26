/// A library of our abstractions over sled database.
///
/// This module is intentionally agnostic to the actual schema of our client and server databases.
/// It implements all the underlying pieces needed to define them, but doesn't assume their exact shape.
pub mod codec;
pub mod key;
pub mod table;
pub mod tree;
