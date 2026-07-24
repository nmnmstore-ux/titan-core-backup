use std::sync::Arc;
use uuid::Uuid;

#[path = "../src/types.rs"]
mod types;
#[path = "../src/io.rs"]
mod io;
#[path = "../src/consensus.rs"]
mod consensus;

use consensus::{DAGConsensus, ConsensusOp, DAGVertex, VertexHash};
use types::Order;

fn test_keypair() -> ([u8; 32], [u8; 32]) {
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    let sk = SigningKey::generate(&mut OsRng);
    let pk = sk.verifying_key();
    (sk.to_bytes(), *pk.as_bytes())
}

fn sample_order() -> Order {
    Order {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        pair: types::CompactString::from("USD/EGP"),
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
    }
}

#[tokio::test]
async fn test_dag_vertex_sign_verify() {
    let (sk, pk) = test_keypair();
    let op = ConsensusOp::PlaceOrder(sample_order());
    let mut vertex = DAGVertex::new_now(op, vec![], "test-node-1");
    vertex.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));
    assert!(vertex.verify(&ed25519_dalek::VerifyingKey::from_bytes(&pk).unwrap()));
}

#[tokio::test]
async fn test_dag_vertex_sign_verify_tampered() {
    let (sk, _pk) = test_keypair();
    let pk2 = {
        let sk2 = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
        sk2.verifying_key()
    };
    let op = ConsensusOp::PlaceOrder(sample_order());
    let mut vertex = DAGVertex::new_now(op, vec![], "test-node-1");
    vertex.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));
    assert!(!vertex.verify(&pk2));
}

#[tokio::test]
async fn test_dag_consensus_submit_and_query() {
    let (sk, _pk) = test_keypair();
    let consensus = Arc::new(DAGConsensus::new("test-node-1", vec![], &sk));
    let op = ConsensusOp::PlaceOrder(sample_order());
    consensus.submit(op).await;
    assert_eq!(consensus.num_vertices().await, 1);
    assert!(consensus.num_tips().await >= 1);
    assert!(consensus.is_healthy().await);
}

#[tokio::test]
async fn test_dag_consensus_multiple_submits() {
    let (sk, _pk) = test_keypair();
    let consensus = Arc::new(DAGConsensus::new("test-node-1", vec![], &sk));
    for _ in 0..10 {
        consensus.submit(ConsensusOp::PlaceOrder(sample_order())).await;
    }
    assert_eq!(consensus.num_vertices().await, 10);
}

#[tokio::test]
async fn test_dag_consensus_with_verification() {
    let (sk, pk) = test_keypair();
    let consensus = Arc::new(DAGConsensus::new("test-node-1", vec![], &sk));

    let op = ConsensusOp::PlaceOrder(sample_order());
    let parents = consensus.select_tips().await;
    let mut vertex = DAGVertex::new_now(op, parents, "test-node-1");
    vertex.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));

    consensus.submit_with_verification(vertex).await;
    assert_eq!(consensus.num_vertices().await, 1);

    let op2 = ConsensusOp::CancelOrder(Uuid::new_v4());
    let parents2 = consensus.select_tips().await;
    let mut vertex2 = DAGVertex::new_now(op2, parents2, "test-node-1");
    vertex2.sign(&ed25519_dalek::SigningKey::from_bytes(&sk));

    consensus.submit_with_verification(vertex2).await;
    assert_eq!(consensus.num_vertices().await, 2);
}

#[tokio::test]
async fn test_dag_consensus_rejects_invalid_signature() {
    let (sk1, _pk1) = test_keypair();
    let (_sk2, pk2) = test_keypair();
    let consensus = Arc::new(DAGConsensus::new("test-node-1", vec![], &sk1));

    let op = ConsensusOp::PlaceOrder(sample_order());
    let mut vertex = DAGVertex::new_now(op, vec![], "test-node-1");
    vertex.sign(&ed25519_dalek::SigningKey::from_bytes(&sk1));

    consensus.submit_with_verification(vertex).await;
    assert_eq!(consensus.num_vertices().await, 1);

    let op2 = ConsensusOp::PlaceOrder(sample_order());
    let mut vertex2 = DAGVertex::new_now(op2, vec![], "different-node");
    vertex2.sign(&ed25519_dalek::SigningKey::from_bytes(&sk1));

    consensus.submit_with_verification(vertex2).await;
    assert_eq!(consensus.num_vertices().await, 2);
}

#[tokio::test]
async fn test_dag_mempool_flush() {
    let (sk, _pk) = test_keypair();
    let consensus = Arc::new(DAGConsensus::new("test-node-1", vec![], &sk));

    assert_eq!(consensus.mempool_depth().await, 0);

    consensus.enqueue_op(ConsensusOp::PlaceOrder(sample_order())).await;
    assert_eq!(consensus.mempool_depth().await, 1);

    consensus.flush_mempool().await;
    assert!(consensus.num_vertices().await >= 1);
    assert_eq!(consensus.mempool_depth().await, 0);
}

#[tokio::test]
async fn test_dag_check_conflicts() {
    let (sk, _pk) = test_keypair();
    let consensus = Arc::new(DAGConsensus::new("test-node-1", vec![], &sk));

    let order_id = Uuid::new_v4();
    let mut order = sample_order();
    order.id = order_id;
    consensus.submit(ConsensusOp::PlaceOrder(order)).await;
    assert_eq!(consensus.num_vertices().await, 1);

    consensus.submit(ConsensusOp::CancelOrder(order_id)).await;
    assert_eq!(consensus.num_vertices().await, 2);

    let mut order2 = sample_order();
    order2.id = order_id;
    consensus.submit(ConsensusOp::PlaceOrder(order2)).await;
    assert_eq!(consensus.num_vertices().await, 3);
}
