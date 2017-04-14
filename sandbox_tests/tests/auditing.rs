#[macro_use]
extern crate exonum;
extern crate sandbox;
extern crate anchoring_btc_service;
#[macro_use]
extern crate anchoring_btc_sandbox;
extern crate serde;
extern crate serde_json;
extern crate bitcoin;
extern crate bitcoinrpc;
extern crate secp256k1;
extern crate blockchain_explorer;
#[macro_use]
extern crate log;

use serde_json::value::ToJson;
use bitcoin::util::base58::ToBase58;
use bitcoin::blockdata::script::Script;
use bitcoin::blockdata::transaction::SigHashType;

use exonum::crypto::{HexValue, Hash};
use exonum::messages::{Message, RawTransaction};
use exonum::storage::StorageValue;
use sandbox::sandbox_tests_helper::{SandboxState, add_one_height_with_transactions,
                                    add_one_height_with_transactions_from_other_validator};
use sandbox::sandbox::Sandbox;
use sandbox::config_updater::TxConfig;

use anchoring_btc_service::details::sandbox::{SandboxClient, Request};
use anchoring_btc_service::details::btc::transactions::{TransactionBuilder, AnchoringTx,
                                                        FundingTx, verify_tx_input};
use anchoring_btc_service::{AnchoringConfig, ANCHORING_SERVICE_ID};
use anchoring_btc_service::blockchain::dto::MsgAnchoringSignature;
use anchoring_btc_sandbox::{AnchoringSandboxState, RpcError, initialize_anchoring_sandbox};
use anchoring_btc_sandbox::helpers::*;
use anchoring_btc_sandbox::secp256k1_hack::sign_tx_input_with_nonce;

fn gen_following_cfg(sandbox: &Sandbox,
                     anchoring_state: &mut AnchoringSandboxState,
                     from_height: u64)
                     -> (RawTransaction, AnchoringConfig) {
    let (_, anchoring_addr) = anchoring_state.common.redeem_script();

    let mut service_cfg = anchoring_state.common.clone();
    let priv_keys = anchoring_state.priv_keys(&anchoring_addr);
    service_cfg.validators.swap_remove(0);

    let following_addr = service_cfg.redeem_script().1;
    for (id, ref mut node) in anchoring_state.nodes.iter_mut().enumerate() {
        node.private_keys
            .insert(following_addr.to_base58check(), priv_keys[id].clone());
    }

    let mut cfg = sandbox.cfg();
    cfg.actual_from = from_height;
    cfg.validators.swap_remove(0);
    *cfg.services
         .get_mut(&ANCHORING_SERVICE_ID.to_string())
         .unwrap() = service_cfg.to_json();
    let tx = TxConfig::new(&sandbox.p(0), &cfg.serialize(), from_height, sandbox.s(0));
    (tx.raw().clone(), service_cfg)
}


// Invoke this method after anchor_first_block_lect_normal
pub fn exclude_node_from_validator(sandbox: &Sandbox,
                                   client: &SandboxClient,
                                   sandbox_state: &mut SandboxState,
                                   anchoring_state: &mut AnchoringSandboxState) {
    let cfg_change_height = 13;
    let (cfg_tx, following_cfg) = gen_following_cfg(&sandbox, anchoring_state, cfg_change_height);
    let (_, following_addr) = following_cfg.redeem_script();

    // Check insufficient confirmations case
    let anchored_tx = anchoring_state.latest_anchored_tx().clone();
    client.expect(vec![
        request! {
            method: "getrawtransaction",
            params: [&anchored_tx.txid(), 1],
            response: {
                "hash":&anchored_tx.txid(),"hex":&anchored_tx.to_hex(),"confirmations": 10,
                "locktime":1088682,"size":223,"txid":"4ae2de1782b19ddab252d88d570f60bc821bd745d031029a8b28f7427c8d0e93","version":1,"vin":[{"scriptSig":{"asm":"3044022075b9f164d9fe44c348c7a18381314c3e6cf22c48e08bacc2ac6e145fd28f73800220448290b7c54ae465a34bb64a1427794428f7d99cc73204a5e501541d07b33e8a[ALL] 02c5f412387bffcc44dec76b28b948bfd7483ec939858c4a65bace07794e97f876","hex":"473044022075b9f164d9fe44c348c7a18381314c3e6cf22c48e08bacc2ac6e145fd28f73800220448290b7c54ae465a34bb64a1427794428f7d99cc73204a5e501541d07b33e8a012102c5f412387bffcc44dec76b28b948bfd7483ec939858c4a65bace07794e97f876"},"sequence":429496729,"txid":"094d7f6acedd8eb4f836ff483157a97155373974ac0ba3278a60e7a0a5efd645","vout":0}],"vout":[{"n":0,"scriptPubKey":{"addresses":["2NDG2AbxE914amqvimARQF2JJBZ9vHDn3Ga"],"asm":"OP_HASH160 db891024f2aa265e3b1998617e8b18ed3b0495fc OP_EQUAL","hex":"a914db891024f2aa265e3b1998617e8b18ed3b0495fc87","reqSigs":1,"type":"scripthash"},"value":0.00004},{"n":1,"scriptPubKey":{"addresses":["mn1jSMdewrpxTDkg1N6brC7fpTNV9X2Cmq"],"asm":"OP_DUP OP_HASH160 474215d1e614a7d9dddbd853d9f139cff2e99e1a OP_EQUALVERIFY OP_CHECKSIG","hex":"76a914474215d1e614a7d9dddbd853d9f139cff2e99e1a88ac","reqSigs":1,"type":"pubkeyhash"},"value":1.00768693}],"vsize":223
            }
        }
    ]);
    add_one_height_with_transactions(&sandbox, &sandbox_state, &[cfg_tx]);

    // Check enough confirmations case
    client.expect(vec![
        request! {
            method: "getrawtransaction",
            params: [&anchored_tx.txid(), 1],
            response: {
                "hash":&anchored_tx.txid(),"hex":&anchored_tx.to_hex(),"confirmations": 100,
                "locktime":1088682,"size":223,"txid":"4ae2de1782b19ddab252d88d570f60bc821bd745d031029a8b28f7427c8d0e93","version":1,"vin":[{"scriptSig":{"asm":"3044022075b9f164d9fe44c348c7a18381314c3e6cf22c48e08bacc2ac6e145fd28f73800220448290b7c54ae465a34bb64a1427794428f7d99cc73204a5e501541d07b33e8a[ALL] 02c5f412387bffcc44dec76b28b948bfd7483ec939858c4a65bace07794e97f876","hex":"473044022075b9f164d9fe44c348c7a18381314c3e6cf22c48e08bacc2ac6e145fd28f73800220448290b7c54ae465a34bb64a1427794428f7d99cc73204a5e501541d07b33e8a012102c5f412387bffcc44dec76b28b948bfd7483ec939858c4a65bace07794e97f876"},"sequence":429496729,"txid":"094d7f6acedd8eb4f836ff483157a97155373974ac0ba3278a60e7a0a5efd645","vout":0}],"vout":[{"n":0,"scriptPubKey":{"addresses":["2NDG2AbxE914amqvimARQF2JJBZ9vHDn3Ga"],"asm":"OP_HASH160 db891024f2aa265e3b1998617e8b18ed3b0495fc OP_EQUAL","hex":"a914db891024f2aa265e3b1998617e8b18ed3b0495fc87","reqSigs":1,"type":"scripthash"},"value":0.00004},{"n":1,"scriptPubKey":{"addresses":["mn1jSMdewrpxTDkg1N6brC7fpTNV9X2Cmq"],"asm":"OP_DUP OP_HASH160 474215d1e614a7d9dddbd853d9f139cff2e99e1a OP_EQUALVERIFY OP_CHECKSIG","hex":"76a914474215d1e614a7d9dddbd853d9f139cff2e99e1a88ac","reqSigs":1,"type":"pubkeyhash"},"value":1.00768693}],"vsize":223
            }
        },
        request! {
            method: "listunspent",
            params: [0, 9999999, [following_addr]],
            response: []
        }
    ]);

    let following_multisig = following_cfg.redeem_script();
    let (_, signatures) =
        anchoring_state.gen_anchoring_tx_with_signatures(&sandbox,
                                                         0,
                                                         anchored_tx.payload().1,
                                                         &[],
                                                         None,
                                                         &following_multisig.1);
    let transition_tx = anchoring_state.latest_anchored_tx().clone();

    add_one_height_with_transactions(&sandbox, &sandbox_state, &[]);
    sandbox.broadcast(signatures[0].clone());

    client.expect(vec![
        request! {
            method: "getrawtransaction",
            params: [&transition_tx.txid(), 1],
            response: {
                "hash":&transition_tx.txid(),"hex":&transition_tx.to_hex(),"confirmations": 0,
                "locktime":1088682,"size":223,"txid":"4ae2de1782b19ddab252d88d570f60bc821bd745d031029a8b28f7427c8d0e93","version":1,"vin":[{"scriptSig":{"asm":"3044022075b9f164d9fe44c348c7a18381314c3e6cf22c48e08bacc2ac6e145fd28f73800220448290b7c54ae465a34bb64a1427794428f7d99cc73204a5e501541d07b33e8a[ALL] 02c5f412387bffcc44dec76b28b948bfd7483ec939858c4a65bace07794e97f876","hex":"473044022075b9f164d9fe44c348c7a18381314c3e6cf22c48e08bacc2ac6e145fd28f73800220448290b7c54ae465a34bb64a1427794428f7d99cc73204a5e501541d07b33e8a012102c5f412387bffcc44dec76b28b948bfd7483ec939858c4a65bace07794e97f876"},"sequence":429496729,"txid":"094d7f6acedd8eb4f836ff483157a97155373974ac0ba3278a60e7a0a5efd645","vout":0}],"vout":[{"n":0,"scriptPubKey":{"addresses":["2NDG2AbxE914amqvimARQF2JJBZ9vHDn3Ga"],"asm":"OP_HASH160 db891024f2aa265e3b1998617e8b18ed3b0495fc OP_EQUAL","hex":"a914db891024f2aa265e3b1998617e8b18ed3b0495fc87","reqSigs":1,"type":"scripthash"},"value":0.00004},{"n":1,"scriptPubKey":{"addresses":["mn1jSMdewrpxTDkg1N6brC7fpTNV9X2Cmq"],"asm":"OP_DUP OP_HASH160 474215d1e614a7d9dddbd853d9f139cff2e99e1a OP_EQUALVERIFY OP_CHECKSIG","hex":"76a914474215d1e614a7d9dddbd853d9f139cff2e99e1a88ac","reqSigs":1,"type":"pubkeyhash"},"value":1.00768693}],"vsize":223
            }
        }
    ]);
    add_one_height_with_transactions(&sandbox, &sandbox_state, &signatures);

    let lects = (0..3)
        .map(|id| {
                 gen_service_tx_lect(&sandbox, id, &transition_tx, 2)
                     .raw()
                     .clone()
             })
        .collect::<Vec<_>>();
    sandbox.broadcast(lects[0].clone());
    add_one_height_with_transactions(&sandbox, &sandbox_state, &lects);

    for _ in sandbox.current_height()..cfg_change_height {
        add_one_height_with_transactions(&sandbox, &sandbox_state, &[]);
    }

    anchoring_state.common = following_cfg;
    add_one_height_with_transactions_from_other_validator(&sandbox, &sandbox_state, &[]);
}

// We exclude sandbox node from validators
// problems: None
// result: success
#[test]
fn test_exclude_node_from_validators() {
    let _ = ::blockchain_explorer::helpers::init_logger();

    let (sandbox, client, mut anchoring_state) = initialize_anchoring_sandbox(&[]);
    let mut sandbox_state = SandboxState::new();

    anchor_first_block(&sandbox, &client, &sandbox_state, &mut anchoring_state);
    anchor_first_block_lect_normal(&sandbox, &client, &sandbox_state, &mut anchoring_state);

    exclude_node_from_validator(&sandbox, &client, &mut sandbox_state, &mut anchoring_state);
}
