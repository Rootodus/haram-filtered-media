// Declare the submodules sitting inside the src/ml/ directory
pub mod engine;
pub mod peopleseg;

// Re-export the clean, public interfaces so external examples can import them effortlessly
pub use engine::init_session;
pub use peopleseg::PeopleSegFilter;
