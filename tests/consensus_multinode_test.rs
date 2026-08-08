#![allow(dead_code, unused_imports)]

use std::sync::Arc;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/pool.rs"]
mod pool;
#[path = "../src/io.rs"]
mod io;
#[path = "../src/time_cache.rs"]
mod time_cache;
#[path = "../src/consensus.rs"]
mod consensus;

use consensus::{DAGConsensus, ConsensusOp, DAGVertex};
use types::Order;

fn test_keypair() -> ([u8; 32], [u8; 32]) {
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    let sk = SigningKey::generate(&mut OsRng);
    let pk = sk.verifying_key();
    (sk.to_bytes(), *pk.as_bytes())
}

fn sample_order() -> Order {
    use uuid::Uuid;
    use compact_str::CompactString;
    Order {
        id: Uuid::new_v4(),
        id_tag: 0,
        user_id: Uuid::new_v4(),
        pair: CompactString::from("USD/EGP"),
        side: types::OrderSide::Buy,
        price: 30.50,
        quantity: 100.0,
        filled: 0.0,
        remaining: 100.0,
        order_type: types::OrderType::Limit,
        status: types::OrderStatus::New,
        timestamp: 0,
        ttl_ms: None,
        is_swap: false,
        swap_target_currency: None,
        tee_signed: false,
        dot_verified: false,
        stealth: false,
        trailing_offset: None,
        trigger_price: None,
        hard_floor: None,
        track: types::Track::Compliant,
        style: types::OrderStyle::Standard,
        hidden_remaining: 0.0,
        client_order_id: None,
        filled_quantity: 0,
    }
}

#[tokio::test]
async fn test_multi_node_vertex_flow() {
    let (sk, pk) = test_keypair();
    let node1 = Arc::new(DAGConsensus::new("node-1", vec![], &sk));
    let node2 = Arc::new(DAGConsensus::new("node-2", vec![], &sk));

    let op1 = ConsensusOp::PlaceOrder(sample_order());
    let parents1 = node1.select_tips().await;
    let mut v1 = DAGVertex::new_now(op1, parents1, "node-1", pk.to_vec());
    v1.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));

    node1.submit_with_verification(v1).await;
    assert_eq!(node1.num_vertices().await, 1);

    let op2 = ConsensusOp::PlaceOrder(sample_order());
    let parents2 = node2.select_tips().await;
    let mut v2 = DAGVertex::new_now(op2, parents2, "node-2", pk.to_vec());
    v2.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));

    node2.submit_with_verification(v2).await;
    assert_eq!(node2.num_vertices().await, 1);

    assert!(node1.is_healthy().await);
    assert!(node2.is_healthy().await);
}

#[tokio::test]
async fn test_multi_node_mempool_independence() {
    let (sk, _pk) = test_keypair();
    let node1 = Arc::new(DAGConsensus::new("node-a", vec![], &sk));
    let node2 = Arc::new(DAGConsensus::new("node-b", vec![], &sk));

    node1.enqueue_op(ConsensusOp::PlaceOrder(sample_order())).await;
    node2.enqueue_op(ConsensusOp::PlaceOrder(sample_order())).await;
    node2.enqueue_op(ConsensusOp::PlaceOrder(sample_order())).await;

    assert_eq!(node1.mempool_depth().await, 1);
    assert_eq!(node2.mempool_depth().await, 2);

    node1.flush_mempool().await;
    node2.flush_mempool().await;

    assert!(node1.num_vertices().await >= 1);
    assert!(node2.num_vertices().await >= 2);
}

#[tokio::test]
async fn test_multi_node_rejects_cross_node_bad_signature() {
    let (sk1, pk1) = test_keypair();
    let (sk2, _pk2) = test_keypair();

    let node1 = Arc::new(DAGConsensus::new("honest-node", vec![], &sk1));
    let op = ConsensusOp::PlaceOrder(sample_order());
    let mut vertex = DAGVertex::new_now(op, vec![], "spoofed-node", pk1.to_vec());
    vertex.sign(&ed25519_dalek::SigningKey::from_bytes(&sk2));

    node1.submit_with_verification(vertex).await;
    assert_eq!(node1.num_vertices().await, 0);
}

#[tokio::test]
async fn test_multi_node_tip_selection() {
    let (sk, pk) = test_keypair();
    let node = Arc::new(DAGConsensus::new("tip-node", vec![], &sk));

    for _ in 0..5 {
        let op = ConsensusOp::PlaceOrder(sample_order());
        let parents = node.select_tips().await;
        let mut v = DAGVertex::new_now(op, parents, "tip-node", pk.to_vec());
        v.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));
        node.submit_with_verification(v).await;
    }

    assert_eq!(node.num_vertices().await, 5);
    assert!(node.num_tips().await >= 1);
}
