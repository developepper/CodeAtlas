//! Contract test suite for the tree-sitter adapter.
//!
//! Uses the shared contract harness from `adapter-api` to verify that
//! the tree-sitter Rust adapter satisfies all behavioral contracts.

use adapter_api::contract::{self, ContractFixture};
use adapter_syntax_treesitter::create_adapter;

// ---------------------------------------------------------------------------
// Aggregate contract suite
// ---------------------------------------------------------------------------

#[test]
fn treesitter_rust_passes_all_contracts() {
    let adapter = create_adapter("rust").expect("rust adapter must be available");
    let fixture = ContractFixture::rust_baseline();
    contract::run_all_contracts(&adapter, &fixture);
}

// ---------------------------------------------------------------------------
// Individual contract tests (for granular failure diagnostics)
// ---------------------------------------------------------------------------

#[test]
fn contract_adapter_identity_is_stable() {
    let adapter = create_adapter("rust").unwrap();
    contract::assert_adapter_identity_is_stable(&adapter);
}

#[test]
fn contract_capabilities_are_valid() {
    let adapter = create_adapter("rust").unwrap();
    contract::assert_capabilities_are_valid(&adapter);
}

#[test]
fn contract_provenance_fields() {
    let adapter = create_adapter("rust").unwrap();
    let fixture = ContractFixture::rust_baseline();
    contract::assert_provenance_fields(&adapter, &fixture);
}

#[test]
fn contract_expected_symbols() {
    let adapter = create_adapter("rust").unwrap();
    let fixture = ContractFixture::rust_baseline();
    contract::assert_expected_symbols(&adapter, &fixture);
}

#[test]
fn contract_symbols_are_valid() {
    let adapter = create_adapter("rust").unwrap();
    let fixture = ContractFixture::rust_baseline();
    contract::assert_symbols_are_valid(&adapter, &fixture);
}

#[test]
fn contract_extraction_is_deterministic() {
    let adapter = create_adapter("rust").unwrap();
    let fixture = ContractFixture::rust_baseline();
    contract::assert_extraction_is_deterministic(&adapter, &fixture);
}

#[test]
fn contract_unsupported_language_rejected() {
    let adapter = create_adapter("rust").unwrap();
    contract::assert_unsupported_language_rejected(&adapter);
}

#[test]
fn contract_empty_file_produces_no_symbols() {
    let adapter = create_adapter("rust").unwrap();
    contract::assert_empty_file_produces_no_symbols(&adapter);
}
