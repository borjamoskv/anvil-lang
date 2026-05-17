// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../contracts/ArchiLedger.sol";

contract ArchiLedgerTest is Test {
    ArchiLedger public ledger;
    address public deployer;
    address public attacker;

    event TraceAnchored(
        bytes32 indexed traceHash,
        string agentId,
        string domain,
        uint256 timestamp,
        address indexed submitter
    );

    function setUp() public {
        deployer = address(this);
        attacker = address(0xBEEF);
        ledger = new ArchiLedger();
    }

    // --- Core: Anchor and Verify ---

    function test_anchorTrace_stores_correctly() public {
        bytes32 hash = keccak256("test_trace_payload_1");

        ledger.anchorTrace(hash, "AGENT_ALPHA", "audit");

        (
            string memory agentId,
            string memory domain,
            uint256 timestamp,
            address submitter
        ) = ledger.traces(hash);

        assertEq(agentId, "AGENT_ALPHA");
        assertEq(domain, "audit");
        assertGt(timestamp, 0);
        assertEq(submitter, deployer);
    }

    function test_verifyTrace_false_before_anchor() public view {
        bytes32 hash = keccak256("nonexistent_trace");
        assertFalse(ledger.verifyTrace(hash));
    }

    function test_verifyTrace_true_after_anchor() public {
        bytes32 hash = keccak256("verified_trace");
        ledger.anchorTrace(hash, "AGENT_BETA", "finance");
        assertTrue(ledger.verifyTrace(hash));
    }

    // --- Replay Protection (Ω₉ Immutability) ---

    function test_revert_on_duplicate_anchor() public {
        bytes32 hash = keccak256("replay_target");
        ledger.anchorTrace(hash, "AGENT_GAMMA", "security");

        vm.expectRevert("Trace already anchored");
        ledger.anchorTrace(hash, "ATTACKER", "replay");
    }

    function test_revert_on_duplicate_from_different_sender() public {
        bytes32 hash = keccak256("cross_sender_replay");
        ledger.anchorTrace(hash, "AGENT_DELTA", "compliance");

        vm.prank(attacker);
        vm.expectRevert("Trace already anchored");
        ledger.anchorTrace(hash, "ATTACKER", "replay");
    }

    // --- Event Emission ---

    function test_emits_TraceAnchored_event() public {
        bytes32 hash = keccak256("event_trace");

        vm.expectEmit(true, true, false, true);
        emit TraceAnchored(
            hash,
            "AGENT_EPSILON",
            "telemetry",
            block.timestamp,
            deployer
        );

        ledger.anchorTrace(hash, "AGENT_EPSILON", "telemetry");
    }

    // --- Multi-trace Independence ---

    function test_multiple_traces_independent() public {
        bytes32 h1 = keccak256("trace_1");
        bytes32 h2 = keccak256("trace_2");
        bytes32 h3 = keccak256("trace_3");

        ledger.anchorTrace(h1, "A1", "d1");
        ledger.anchorTrace(h2, "A2", "d2");
        ledger.anchorTrace(h3, "A3", "d3");

        assertTrue(ledger.verifyTrace(h1));
        assertTrue(ledger.verifyTrace(h2));
        assertTrue(ledger.verifyTrace(h3));

        (string memory id1,,, ) = ledger.traces(h1);
        (string memory id2,,, ) = ledger.traces(h2);
        (string memory id3,,, ) = ledger.traces(h3);

        assertEq(id1, "A1");
        assertEq(id2, "A2");
        assertEq(id3, "A3");
    }

    // --- Edge: Empty Strings ---

    function test_anchor_with_empty_strings() public {
        bytes32 hash = keccak256("empty_fields");
        ledger.anchorTrace(hash, "", "");

        (string memory agentId, string memory domain,, ) = ledger.traces(hash);
        assertEq(agentId, "");
        assertEq(domain, "");
        assertTrue(ledger.verifyTrace(hash));
    }

    // --- Fuzz: Random Hash Anchoring ---

    function testFuzz_anchor_and_verify(bytes32 hash) public {
        // Skip zero hash (would fail verifyTrace logic since timestamp=0 check)
        vm.assume(hash != bytes32(0));

        ledger.anchorTrace(hash, "FUZZ_AGENT", "fuzz");
        assertTrue(ledger.verifyTrace(hash));
    }
}
