use crate::lumalanghooks::bitcoin::BitcoinLumalanghookInstance;
use crate::lumalanghooks::bitcoin::BitcoinLumalanghookSpecification;
use crate::lumalanghooks::bitcoin::BitcoinLumalanghookSpecificationNetworkMap;
use crate::lumalanghooks::bitcoin::BitcoinPredicateType;
use crate::lumalanghooks::bitcoin::InscriptionFeedData;
use crate::lumalanghooks::bitcoin::OrdinalOperations;
use crate::lumalanghooks::bitcoin::OutputPredicate;
use crate::lumalanghooks::stacks::StacksLumalanghookInstance;
use crate::lumalanghooks::stacks::StacksLumalanghookSpecification;
use crate::lumalanghooks::stacks::StacksLumalanghookSpecificationNetworkMap;
use crate::lumalanghooks::stacks::StacksContractCallBasedPredicate;
use crate::lumalanghooks::stacks::StacksPredicate;
use crate::lumalanghooks::types::{
    LumalanghookInstance, LumalanghookSpecificationNetworkMap, LumalanghookStore, ExactMatchingRule,
    HookAction,
};
use crate::indexer::fork_scratch_pad::ForkScratchPad;
use crate::indexer::tests::helpers::transactions::generate_test_tx_bitcoin_p2pkh_transfer;
use crate::indexer::tests::helpers::{
    accounts, bitcoin_blocks, stacks_blocks, transactions::generate_test_tx_stacks_contract_call,
};
use crate::monitoring::PrometheusMonitoring;
use crate::observer::PredicateDeregisteredEvent;
use crate::observer::{
    start_observer_commands_handler, EventObserverConfig, ObserverCommand, ObserverSidecar,
};
use crate::utils::{AbstractBlock, Context};
use lumalanghook_types::OrdinalInscriptionCharms;
use lumalanghook_types::{
    BitcoinBlockSignaling, BitcoinChainEvent, BitcoinNetwork, BlockchainEvent,
    BlockchainUpdatedWithHeaders, OrdinalInscriptionNumber, OrdinalInscriptionRevealData,
    OrdinalOperation, StacksBlockUpdate, StacksChainEvent, StacksChainUpdatedWithBlocksData,
    StacksNetwork, StacksNodeConfig,
};
use hiro_system_kit;
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Sender};

use super::PredicatesConfig;
use super::{ObserverEvent, DEFAULT_INGESTION_PORT};

fn generate_test_config() -> (EventObserverConfig, LumalanghookStore) {
    let config: EventObserverConfig = EventObserverConfig {
        registered_lumalanghooks: LumalanghookStore::new(),
        predicates_config: PredicatesConfig::default(),
        bitcoin_rpc_proxy_enabled: false,
        bitcoind_rpc_username: "user".into(),
        bitcoind_rpc_password: "user".into(),
        bitcoind_rpc_url: "http://localhost:18443".into(),
        display_stacks_ingestion_logs: false,
        bitcoin_block_signaling: BitcoinBlockSignaling::Stacks(
            StacksNodeConfig::default_localhost(DEFAULT_INGESTION_PORT),
        ),
        bitcoin_network: BitcoinNetwork::Regtest,
        stacks_network: StacksNetwork::Devnet,
        prometheus_monitoring_port: None,
    };
    (config, LumalanghookStore::new())
}

fn stacks_lumalanghook_contract_call(
    id: u8,
    contract_identifier: &str,
    expire_after_occurrence: Option<u64>,
    method: &str,
) -> StacksLumalanghookSpecificationNetworkMap {
    let mut networks = BTreeMap::new();
    networks.insert(
        StacksNetwork::Devnet,
        StacksLumalanghookSpecification {
            start_block: None,
            end_block: None,
            blocks: None,
            expire_after_occurrence,
            capture_all_events: None,
            decode_clarity_values: Some(true),
            include_contract_abi: None,
            predicate: StacksPredicate::ContractCall(StacksContractCallBasedPredicate {
                contract_identifier: contract_identifier.to_string(),
                method: method.to_string(),
            }),
            action: HookAction::Noop,
        },
    );

    
    StacksLumalanghookSpecificationNetworkMap {
        uuid: format!("{}", id),
        name: format!("LumaLang {}", id),
        owner_uuid: None,
        networks,
        version: 1,
    }
}

fn bitcoin_lumalanghook_p2pkh(
    id: u8,
    address: &str,
    expire_after_occurrence: Option<u64>,
) -> BitcoinLumalanghookSpecificationNetworkMap {
    let mut networks = BTreeMap::new();
    networks.insert(
        BitcoinNetwork::Regtest,
        BitcoinLumalanghookSpecification {
            start_block: None,
            end_block: None,
            blocks: None,
            expire_after_occurrence,
            predicate: BitcoinPredicateType::Outputs(OutputPredicate::P2pkh(
                ExactMatchingRule::Equals(address.to_string()),
            )),
            action: HookAction::Noop,
            include_proof: None,
            include_inputs: None,
            include_outputs: None,
            include_witness: None,
        },
    );

    
    BitcoinLumalanghookSpecificationNetworkMap {
        uuid: format!("{}", id),
        name: format!("LumaLang {}", id),
        owner_uuid: None,
        version: 1,
        networks,
    }
}

fn bitcoin_lumalanghook_ordinals(id: u8) -> BitcoinLumalanghookSpecificationNetworkMap {
    let mut networks = BTreeMap::new();
    networks.insert(
        BitcoinNetwork::Regtest,
        BitcoinLumalanghookSpecification {
            start_block: None,
            end_block: None,
            blocks: None,
            expire_after_occurrence: None,
            predicate: BitcoinPredicateType::OrdinalsProtocol(OrdinalOperations::InscriptionFeed(
                InscriptionFeedData {
                    meta_protocols: None,
                },
            )),
            action: HookAction::Noop,
            include_proof: None,
            include_inputs: None,
            include_outputs: None,
            include_witness: None,
        },
    );

    
    BitcoinLumalanghookSpecificationNetworkMap {
        uuid: format!("{}", id),
        name: format!("LumaLang {}", id),
        owner_uuid: None,
        version: 1,
        networks,
    }
}

fn generate_and_register_new_stacks_lumalanghook(
    observer_commands_tx: &Sender<ObserverCommand>,
    observer_events_rx: &crossbeam_channel::Receiver<ObserverEvent>,
    id: u8,
    contract_name: &str,
    method: &str,
) -> StacksLumalanghookInstance {
    let contract_identifier = format!("{}.{}", accounts::deployer_stx_address(), contract_name);
    let lumaLang = stacks_lumalanghook_contract_call(id, &contract_identifier, None, method);
    let _ = observer_commands_tx.send(ObserverCommand::RegisterPredicate(
        LumalanghookSpecificationNetworkMap::Stacks(lumaLang.clone()),
    ));
    let mut lumaLang = lumaLang
        .into_specification_for_network(&StacksNetwork::Devnet)
        .unwrap();
    lumaLang.enabled = true;
    let _ = observer_commands_tx.send(ObserverCommand::EnablePredicate(LumalanghookInstance::Stacks(
        lumaLang.clone(),
    )));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateRegistered(_)) => {
            // assert_eq!(
            //     LumalanghookSpecification::Stacks(lumaLang.clone()),
            //     registered_lumalanghook
            // );
            true
        }
        _ => false,
    });
    let _ = observer_commands_tx.send(ObserverCommand::EnablePredicate(LumalanghookInstance::Stacks(
        lumaLang.clone(),
    )));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateEnabled(_)) => {
            // assert_eq!(
            //     LumalanghookSpecification::Bitcoin(lumaLang.clone()),
            //     registered_lumalanghook
            // );
            true
        }
        _ => false,
    });
    lumaLang
}

fn generate_and_register_new_bitcoin_lumalanghook(
    observer_commands_tx: &Sender<ObserverCommand>,
    observer_events_rx: &crossbeam_channel::Receiver<ObserverEvent>,
    id: u8,
    p2pkh_address: &str,
    expire_after_occurrence: Option<u64>,
) -> BitcoinLumalanghookInstance {
    let lumaLang = bitcoin_lumalanghook_p2pkh(id, p2pkh_address, expire_after_occurrence);
    let _ = observer_commands_tx.send(ObserverCommand::RegisterPredicate(
        LumalanghookSpecificationNetworkMap::Bitcoin(lumaLang.clone()),
    ));
    let mut lumaLang = lumaLang
        .into_specification_for_network(&BitcoinNetwork::Regtest)
        .unwrap();
    lumaLang.enabled = true;
    let _ = observer_commands_tx.send(ObserverCommand::EnablePredicate(
        LumalanghookInstance::Bitcoin(lumaLang.clone()),
    ));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateRegistered(_)) => {
            // assert_eq!(
            //     LumalanghookSpecification::Bitcoin(lumaLang.clone()),
            //     registered_lumalanghook
            // );
            true
        }
        _ => false,
    });
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateEnabled(_)) => {
            // assert_eq!(
            //     LumalanghookSpecification::Bitcoin(lumaLang.clone()),
            //     registered_lumalanghook
            // );
            true
        }
        _ => false,
    });
    lumaLang
}

fn assert_predicates_triggered_event(
    observer_events_rx: &crossbeam_channel::Receiver<ObserverEvent>,
    expected_len: usize,
) {
    assert!(
        match observer_events_rx.recv() {
            Ok(ObserverEvent::PredicatesTriggered(len)) => {
                assert_eq!(
                    len, expected_len,
                    "expected {} predicate(s) to be triggered",
                    expected_len
                );
                true
            }
            _ => false,
        },
        "expected PredicatesTriggered event to occur"
    );
}

fn assert_stacks_chain_event(observer_events_rx: &crossbeam_channel::Receiver<ObserverEvent>) {
    assert!(
        match observer_events_rx.recv() {
            Ok(ObserverEvent::StacksChainEvent(_)) => {
                true
            }
            _ => false,
        },
        "expected StacksChainEvent event to occur"
    );
}

fn assert_observer_metrics_stacks_registered_predicates(
    prometheus_monitoring: &PrometheusMonitoring,
    expected_count: u64,
) {
    assert_eq!(
        expected_count,
        prometheus_monitoring.stx_registered_predicates.get(),
        "expected {} registered stacks hooks",
        expected_count
    );
}

fn assert_observer_metrics_stacks_deregistered_predicates(
    prometheus_monitoring: &PrometheusMonitoring,
    expected_count: u64,
) {
    assert_eq!(
        expected_count,
        prometheus_monitoring.stx_deregistered_predicates.get(),
        "expected {} deregistered stacks hooks",
        expected_count
    );
}

fn assert_observer_metrics_bitcoin_registered_predicates(
    prometheus_monitoring: &PrometheusMonitoring,
    expected_count: u64,
) {
    assert_eq!(
        expected_count,
        prometheus_monitoring.btc_registered_predicates.get(),
        "expected {} registered bitcoin hooks",
        expected_count
    );
}

fn assert_observer_metrics_bitcoin_deregistered_predicates(
    prometheus_monitoring: &PrometheusMonitoring,
    expected_count: u64,
) {
    assert_eq!(
        expected_count,
        prometheus_monitoring.btc_deregistered_predicates.get(),
        "expected {} deregistered bitcoin hooks",
        expected_count
    );
}

fn generate_and_register_new_ordinals_lumalanghook(
    observer_commands_tx: &Sender<ObserverCommand>,
    observer_events_rx: &crossbeam_channel::Receiver<ObserverEvent>,
    id: u8,
) -> BitcoinLumalanghookInstance {
    let lumaLang = bitcoin_lumalanghook_ordinals(id);
    let _ = observer_commands_tx.send(ObserverCommand::RegisterPredicate(
        LumalanghookSpecificationNetworkMap::Bitcoin(lumaLang.clone()),
    ));
    let mut lumaLang = lumaLang
        .into_specification_for_network(&BitcoinNetwork::Regtest)
        .unwrap();
    lumaLang.enabled = true;
    let _ = observer_commands_tx.send(ObserverCommand::EnablePredicate(
        LumalanghookInstance::Bitcoin(lumaLang.clone()),
    ));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateRegistered(_)) => {
            true
        }
        _ => false,
    });
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateEnabled(_)) => {
            true
        }
        _ => false,
    });
    lumaLang
}

#[test]
fn test_stacks_lumalanghook_register_deregister() {
    let (observer_commands_tx, observer_commands_rx) = channel();
    let (observer_events_tx, observer_events_rx) = crossbeam_channel::unbounded();
    let prometheus_monitoring = PrometheusMonitoring::new();
    let prometheus_monitoring_moved = prometheus_monitoring.clone();

    let handle = std::thread::spawn(move || {
        let (config, lumalanghook_store) = generate_test_config();
        let _ = hiro_system_kit::nestable_block_on(start_observer_commands_handler(
            config,
            lumalanghook_store,
            observer_commands_rx,
            Some(observer_events_tx),
            None,
            prometheus_monitoring_moved,
            None,
            Context::empty(),
        ));
    });

    // Create and register a new lumaLang
    let lumaLang = generate_and_register_new_stacks_lumalanghook(
        &observer_commands_tx,
        &observer_events_rx,
        1,
        "counter",
        "increment",
    );

    // registering stacks lumaLang should increment the observer_metric's registered stacks hooks
    assert_observer_metrics_stacks_registered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        0,
        &accounts::wallet_1_stx_address(),
        "counter",
        "decrement",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 1, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateEnabled(_)) => true,
        _ => false,
    });

    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);
    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    // Simulate a block that does include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        1,
        &accounts::wallet_1_stx_address(),
        "counter",
        "increment",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 2, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 1);

    assert!(
        match observer_events_rx.recv() {
            Ok(ObserverEvent::StacksPredicateTriggered(payload)) => {
                assert_eq!(
                    payload.apply.len(),
                    1,
                    "expected 1 predicate to be triggered"
                );
                assert_eq!(
                    payload.apply[0].transactions.len(),
                    1,
                    "expected triggered predicate to have 1 transaction"
                );
                true
            }
            _ => false,
        },
        "expected StacksPredicateTriggered event to occur"
    );

    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    // Simulate a block that does include 2 trigger
    let transactions = vec![
        generate_test_tx_stacks_contract_call(
            1,
            &accounts::wallet_1_stx_address(),
            "counter",
            "increment",
            vec!["u1"],
        ),
        generate_test_tx_stacks_contract_call(
            2,
            &accounts::wallet_2_stx_address(),
            "counter",
            "increment",
            vec!["u2"],
        ),
        generate_test_tx_stacks_contract_call(
            3,
            &accounts::wallet_3_stx_address(),
            "counter",
            "decrement",
            vec!["u2"],
        ),
    ];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 2, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 1);

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::StacksPredicateTriggered(payload)) => {
            assert_eq!(payload.apply.len(), 1);
            assert_eq!(payload.apply[0].transactions.len(), 2);
            true
        }
        _ => false,
    });

    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    // Deregister the hook
    let _ = observer_commands_tx.send(ObserverCommand::DeregisterStacksPredicate(
        lumaLang.uuid.clone(),
    ));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateDeregistered(PredicateDeregisteredEvent {
            predicate_uuid: deregistered_lumalanghook,
            ..
        })) => {
            assert_eq!(lumaLang.uuid, deregistered_lumalanghook);
            true
        }
        _ => false,
    });

    // deregistering stacks lumaLang should decrement the observer_metric's registered stacks hooks
    assert_observer_metrics_stacks_registered_predicates(&prometheus_monitoring, 0);
    // and increment the deregistered hooks
    assert_observer_metrics_stacks_deregistered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        2,
        &accounts::wallet_1_stx_address(),
        "counter",
        "decrement",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 2, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);
    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    // Simulate a block that does include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        3,
        &accounts::wallet_1_stx_address(),
        "counter",
        "increment",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 3, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);
    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    let _ = observer_commands_tx.send(ObserverCommand::Terminate);
    handle.join().expect("unable to terminate thread");
}

#[test]
fn test_stacks_lumalanghook_auto_deregister() {
    let (observer_commands_tx, observer_commands_rx) = channel();
    let (observer_events_tx, observer_events_rx) = crossbeam_channel::unbounded();
    let prometheus_monitoring = PrometheusMonitoring::new();
    let prometheus_monitoring_moved = prometheus_monitoring.clone();

    let handle = std::thread::spawn(move || {
        let (config, lumalanghook_store) = generate_test_config();
        let _ = hiro_system_kit::nestable_block_on(start_observer_commands_handler(
            config,
            lumalanghook_store,
            observer_commands_rx,
            Some(observer_events_tx),
            None,
            prometheus_monitoring_moved,
            None,
            Context::empty(),
        ));
    });

    // Create and register a new lumaLang
    let contract_identifier = format!("{}.{}", accounts::deployer_stx_address(), "counter");
    let lumaLang = stacks_lumalanghook_contract_call(0, &contract_identifier, Some(1), "increment");
    let _ = observer_commands_tx.send(ObserverCommand::RegisterPredicate(
        LumalanghookSpecificationNetworkMap::Stacks(lumaLang.clone()),
    ));
    let mut lumaLang = lumaLang
        .into_specification_for_network(&StacksNetwork::Devnet)
        .unwrap();
    lumaLang.enabled = true;
    let _ = observer_commands_tx.send(ObserverCommand::EnablePredicate(LumalanghookInstance::Stacks(
        lumaLang.clone(),
    )));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateRegistered(_)) => {
            // assert_eq!(
            //     LumalanghookSpecification::Stacks(lumaLang.clone()),
            //     registered_lumalanghook
            // );
            true
        }
        _ => false,
    });
    // registering stacks lumaLang should increment the observer_metric's registered stacks hooks
    assert_observer_metrics_stacks_registered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        0,
        &accounts::wallet_1_stx_address(),
        "counter",
        "decrement",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 1, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateEnabled(_)) => true,
        _ => false,
    });

    assert!(
        match observer_events_rx.recv() {
            Ok(ObserverEvent::PredicatesTriggered(len)) => {
                assert_eq!(len, 0);
                true
            }
            Ok(e) => {
                println!("{:?}", e);
                true
            }
            _ => false,
        },
        "expected PredicatesTriggered event to occur"
    );
    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    // Simulate a block that does include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        1,
        &accounts::wallet_1_stx_address(),
        "counter",
        "increment",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 2, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that hooks were triggered
    assert_predicates_triggered_event(&observer_events_rx, 1);

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::StacksPredicateTriggered(_)) => {
            true
        }
        _ => false,
    });

    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    // Simulate another block that does include a trigger
    let transactions = vec![generate_test_tx_stacks_contract_call(
        3,
        &accounts::wallet_1_stx_address(),
        "counter",
        "increment",
        vec!["u1"],
    )];
    let chain_event = StacksChainEvent::ChainUpdatedWithBlocks(StacksChainUpdatedWithBlocksData {
        new_blocks: vec![StacksBlockUpdate::new(
            stacks_blocks::generate_test_stacks_block(0, 3, transactions, None).expect_block(),
        )],
        confirmed_blocks: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateStacksChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should signal that a hook was deregistered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateDeregistered(PredicateDeregisteredEvent {
            predicate_uuid: deregistered_hook,
            ..
        })) => {
            assert_eq!(deregistered_hook, lumaLang.uuid);
            true
        }
        _ => false,
    });

    // deregistering stacks lumaLang should decrement the observer_metric's registered stacks hooks
    assert_observer_metrics_stacks_registered_predicates(&prometheus_monitoring, 0);
    // and increment the deregistered hooks
    assert_observer_metrics_stacks_deregistered_predicates(&prometheus_monitoring, 1);

    // Should propagate block
    assert_stacks_chain_event(&observer_events_rx);

    let _ = observer_commands_tx.send(ObserverCommand::Terminate);
    handle.join().expect("unable to terminate thread");
}

#[test]
fn test_bitcoin_lumalanghook_register_deregister() {
    let (observer_commands_tx, observer_commands_rx) = channel();
    let (observer_events_tx, observer_events_rx) = crossbeam_channel::unbounded();
    let prometheus_monitoring = PrometheusMonitoring::new();
    let prometheus_monitoring_moved = prometheus_monitoring.clone();

    let handle = std::thread::spawn(move || {
        let (config, lumalanghook_store) = generate_test_config();
        let _ = hiro_system_kit::nestable_block_on(start_observer_commands_handler(
            config,
            lumalanghook_store,
            observer_commands_rx,
            Some(observer_events_tx),
            None,
            prometheus_monitoring_moved,
            None,
            Context::empty(),
        ));
    });

    // Create and register a new lumaLang (wallet_2 received some sats)
    let lumaLang = generate_and_register_new_bitcoin_lumalanghook(
        &observer_commands_tx,
        &observer_events_rx,
        1,
        &accounts::wallet_2_btc_address(),
        None,
    );

    // registering bitcoin lumaLang should increment the observer_metric's registered bitcoin hooks
    assert_observer_metrics_bitcoin_registered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger (wallet_1 to wallet_3)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_3_btc_address(),
        3,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 1, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger (wallet_1 to wallet_2)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_2_btc_address(),
        3,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });

    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));
    // Should signal that 1 hook was triggered
    assert_predicates_triggered_event(&observer_events_rx, 1);

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinPredicateTriggered(payload)) => {
            assert_eq!(payload.apply.len(), 1);
            assert_eq!(payload.apply[0].block.transactions.len(), 1);
            true
        }
        _ => false,
    });

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger (wallet_1 to wallet_2)
    let transactions = vec![
        generate_test_tx_bitcoin_p2pkh_transfer(
            0,
            &accounts::wallet_1_btc_address(),
            &accounts::wallet_2_btc_address(),
            3,
        ),
        generate_test_tx_bitcoin_p2pkh_transfer(
            1,
            &accounts::wallet_3_btc_address(),
            &accounts::wallet_2_btc_address(),
            5,
        ),
        generate_test_tx_bitcoin_p2pkh_transfer(
            1,
            &accounts::wallet_3_btc_address(),
            &accounts::wallet_1_btc_address(),
            5,
        ),
    ];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });

    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));
    // Should signal that 1 hook was triggered
    assert_predicates_triggered_event(&observer_events_rx, 1);

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinPredicateTriggered(payload)) => {
            assert_eq!(payload.apply.len(), 1);
            assert_eq!(payload.apply[0].block.transactions.len(), 2);
            true
        }
        _ => false,
    });

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Deregister the hook
    let _ = observer_commands_tx.send(ObserverCommand::DeregisterBitcoinPredicate(
        lumaLang.uuid.clone(),
    ));
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateDeregistered(PredicateDeregisteredEvent {
            predicate_uuid: deregistered_lumalanghook,
            ..
        })) => {
            assert_eq!(lumaLang.uuid, deregistered_lumalanghook);
            true
        }
        _ => false,
    });

    // deregistering bitcoin lumaLang should decrement the observer_metric's registered bitcoin hooks
    assert_observer_metrics_bitcoin_registered_predicates(&prometheus_monitoring, 0);
    // and increment the deregistered hooks
    assert_observer_metrics_bitcoin_deregistered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        2,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_3_btc_address(),
        1,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        3,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_2_btc_address(),
        1,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 3, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    let _ = observer_commands_tx.send(ObserverCommand::Terminate);
    handle.join().expect("unable to terminate thread");
}

#[test]
fn test_bitcoin_lumalanghook_auto_deregister() {
    let (observer_commands_tx, observer_commands_rx) = channel();
    let (observer_events_tx, observer_events_rx) = crossbeam_channel::unbounded();
    let prometheus_monitoring = PrometheusMonitoring::new();
    let prometheus_monitoring_moved = prometheus_monitoring.clone();

    let handle = std::thread::spawn(move || {
        let (config, lumalanghook_store) = generate_test_config();
        let _ = hiro_system_kit::nestable_block_on(start_observer_commands_handler(
            config,
            lumalanghook_store,
            observer_commands_rx,
            Some(observer_events_tx),
            None,
            prometheus_monitoring_moved,
            None,
            Context::empty(),
        ));
    });

    // Create and register a new lumaLang (wallet_2 received some sats)
    let lumaLang = generate_and_register_new_bitcoin_lumalanghook(
        &observer_commands_tx,
        &observer_events_rx,
        1,
        &accounts::wallet_2_btc_address(),
        Some(1),
    );

    // registering bitcoin lumaLang should increment the observer_metric's registered bitcoin hooks
    assert_observer_metrics_bitcoin_registered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger (wallet_1 to wallet_3)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_3_btc_address(),
        3,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 1, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger (wallet_1 to wallet_2)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_2_btc_address(),
        3,
    )];

    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that 1 hook was triggered
    assert_predicates_triggered_event(&observer_events_rx, 1);

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinPredicateTriggered(_)) => {
            true
        }
        _ => false,
    });

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does not include a trigger
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        2,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_3_btc_address(),
        1,
    )];

    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        3,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_2_btc_address(),
        1,
    )];

    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 3, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = BlockchainEvent::BlockchainUpdatedWithHeaders(BlockchainUpdatedWithHeaders {
        new_headers: vec![block.get_header()],
        confirmed_headers: vec![],
    });
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that no hook were triggered
    assert_predicates_triggered_event(&observer_events_rx, 0);

    // Should signal that a hook was deregistered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicateDeregistered(PredicateDeregisteredEvent {
            predicate_uuid: deregistered_hook,
            ..
        })) => {
            assert_eq!(deregistered_hook, lumaLang.uuid);
            true
        }
        _ => false,
    });

    // deregistering bitcoin lumaLang should decrement the observer_metric's registered bitcoin hooks
    assert_observer_metrics_bitcoin_registered_predicates(&prometheus_monitoring, 0);
    // and increment the deregistered hooks
    assert_observer_metrics_bitcoin_deregistered_predicates(&prometheus_monitoring, 1);

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent(_)) => {
            true
        }
        _ => false,
    });

    let _ = observer_commands_tx.send(ObserverCommand::Terminate);
    handle.join().expect("unable to terminate thread");
}

#[test]
fn test_bitcoin_lumalanghook_through_reorg() {
    let (observer_commands_tx, observer_commands_rx) = channel();
    let (block_pre_processor_in_tx, block_pre_processor_in_rx) = crossbeam_channel::unbounded();
    let (block_pre_processor_out_tx, block_pre_processor_out_rx) = crossbeam_channel::unbounded();

    let (observer_events_tx, observer_events_rx) = crossbeam_channel::unbounded();

    let empty_ctx = Context::empty();

    let observer_sidecar = ObserverSidecar {
        bitcoin_blocks_mutator: Some((block_pre_processor_in_tx, block_pre_processor_out_rx)),
        bitcoin_chain_event_notifier: None,
    };
    let prometheus_monitoring = PrometheusMonitoring::new();
    let prometheus_monitoring_moved = prometheus_monitoring.clone();

    let handle = std::thread::spawn(move || {
        let (config, lumalanghook_store) = generate_test_config();
        let _ = hiro_system_kit::nestable_block_on(start_observer_commands_handler(
            config,
            lumalanghook_store,
            observer_commands_rx,
            Some(observer_events_tx),
            None,
            prometheus_monitoring_moved,
            Some(observer_sidecar),
            Context::empty(),
        ));
    });

    // The block pre-processor will simulate block augmentation with new informations, which should trigger
    // registered predicates
    let block_pre_processor_handle = std::thread::spawn(move || {
        let mut inscription_number = OrdinalInscriptionNumber::zero();
        let mut cursor = 0;
        while let Ok((mut blocks, _)) = block_pre_processor_in_rx.recv() {
            for b in blocks.iter_mut() {
                for (tx_index, tx) in b.block.transactions.iter_mut().enumerate() {
                    cursor += 1;
                    inscription_number.classic += 1;
                    inscription_number.jubilee += 1;
                    tx.metadata
                        .ordinal_operations
                        .push(OrdinalOperation::InscriptionRevealed(
                            OrdinalInscriptionRevealData {
                                content_bytes: format!("{cursor}"),
                                content_type: "".to_string(),
                                content_length: cursor as usize,
                                inscription_number: inscription_number.clone(),
                                inscription_fee: cursor,
                                inscription_output_value: cursor,
                                inscription_id: format!("{cursor}"),
                                inscription_input_index: 0,
                                inscription_pointer: None,
                                inscriber_address: None,
                                metadata: None,
                                metaprotocol: None,
                                delegate: None,
                                parents: vec![],
                                ordinal_number: cursor,
                                ordinal_block_height: b.block.block_identifier.index,
                                ordinal_offset: 0,
                                tx_index,
                                transfers_pre_inscription: cursor as u32,
                                satpoint_post_inscription: format!("{cursor}"),
                                curse_type: None,
                                charms: OrdinalInscriptionCharms::none(),
                            },
                        ))
                }
            }
            let _ = block_pre_processor_out_tx.send(blocks);
        }
    });

    let genesis = bitcoin_blocks::generate_test_bitcoin_block(0, 1, vec![], None);
    let mut fork_pad = ForkScratchPad::new();
    let _ = fork_pad.process_header(genesis.get_header(), &empty_ctx);

    // Create and register a new lumaLang (wallet_2 received some sats)
    let _lumalanghook =
        generate_and_register_new_ordinals_lumalanghook(&observer_commands_tx, &observer_events_rx, 1);

    // registering bitcoin lumaLang should increment the observer_metric's registered bitcoin hooks

    assert_observer_metrics_bitcoin_registered_predicates(&prometheus_monitoring, 1);

    // Simulate a block that does not include a trigger (wallet_1 to wallet_3)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_3_btc_address(),
        3,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));

    let chain_event = fork_pad
        .process_header(block.get_header(), &empty_ctx)
        .unwrap()
        .unwrap();
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));
    // Should signal that no hook were triggered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicatesTriggered(len)) => {
            assert_eq!(len, 1);
            true
        }
        Ok(event) => {
            println!("Unexpected event: {:?}", event);
            false
        }
        Err(e) => {
            println!("Error: {:?}", e);
            false
        }
    });

    // 1) Should kick off predicate event
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinPredicateTriggered(payload)) => {
            assert_eq!(payload.apply.len(), 1);
            assert_eq!(payload.apply[0].block.transactions.len(), 1);
            true
        }
        _ => false,
    });

    // 2) Should kick off bitcoin chain event
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent((BitcoinChainEvent::ChainUpdatedWithBlocks(_), _))) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger (wallet_1 to wallet_2)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_2_btc_address(),
        3,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(1, 2, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = fork_pad
        .process_header(block.get_header(), &empty_ctx)
        .unwrap()
        .unwrap();
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that no hook were triggered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicatesTriggered(len)) => {
            assert_eq!(len, 1);
            true
        }
        _ => false,
    });

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinPredicateTriggered(payload)) => {
            assert_eq!(payload.rollback.len(), 1);
            assert_eq!(payload.rollback[0].block.transactions.len(), 1);
            assert_eq!(payload.apply.len(), 1);
            assert_eq!(payload.apply[0].block.transactions.len(), 1);
            true
        }
        _ => false,
    });

    // Should propagate chain event
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent((BitcoinChainEvent::ChainUpdatedWithReorg(_), _))) => {
            true
        }
        _ => false,
    });

    // Simulate a block that does include a trigger (wallet_1 to wallet_2)
    let transactions = vec![generate_test_tx_bitcoin_p2pkh_transfer(
        0,
        &accounts::wallet_1_btc_address(),
        &accounts::wallet_2_btc_address(),
        3,
    )];
    let block = bitcoin_blocks::generate_test_bitcoin_block(0, 3, transactions, None);
    let _ = observer_commands_tx.send(ObserverCommand::CacheBitcoinBlock(block.clone()));
    let chain_event = fork_pad
        .process_header(block.get_header(), &empty_ctx)
        .unwrap()
        .unwrap();
    let _ = observer_commands_tx.send(ObserverCommand::PropagateBitcoinChainEvent(chain_event));

    // Should signal that no hook were triggered
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::PredicatesTriggered(len)) => {
            assert_eq!(len, 1);
            true
        }
        _ => false,
    });

    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinPredicateTriggered(payload)) => {
            assert_eq!(payload.rollback.len(), 1);
            assert_eq!(payload.rollback[0].block.transactions.len(), 1);
            assert_eq!(payload.apply.len(), 2);
            assert_eq!(payload.apply[0].block.transactions.len(), 1);
            true
        }
        _ => false,
    });

    // Should propagate block
    assert!(match observer_events_rx.recv() {
        Ok(ObserverEvent::BitcoinChainEvent((BitcoinChainEvent::ChainUpdatedWithReorg(_), _))) => {
            true
        }
        _ => false,
    });

    let _ = observer_commands_tx.send(ObserverCommand::Terminate);
    handle.join().expect("unable to terminate thread");
    block_pre_processor_handle
        .join()
        .expect("unable to terminate thread");
}
