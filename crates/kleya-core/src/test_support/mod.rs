//! Fakes for testing kleya-core commands without a real provider.

#![allow(missing_docs, clippy::expect_used, clippy::disallowed_methods)]

pub mod fake_clock;
pub mod fake_id_gen;
pub mod in_memory_compute;
pub mod in_memory_key_store;

pub use fake_clock::FakeClock;
pub use fake_id_gen::FakeIdGen;
pub use in_memory_compute::InMemoryCompute;
pub use in_memory_key_store::InMemoryKeyStore;
