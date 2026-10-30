//! Per-station hash-chained JSONL logs: append then fsync, verify, torn-
//! write repair, and DAG heads.
//!
//! Milestone 0 skeleton. The implementation lands in Milestone 1; see
//! section 11 of docs/V1_DESIGN_SPEC.md.
