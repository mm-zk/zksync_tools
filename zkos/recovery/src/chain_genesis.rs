// Things related to genesis state and upgrade.

use std::collections::BTreeMap;

use alloy::{
    dyn_abi::SolType,
    primitives::{Address, address},
    providers::Provider,
    rpc::types::Filter,
    sol_types::{SolCall, SolEvent},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    bytecodes::{BytecodeInfo, analyze_bytecode},
    contracts::{
        FixedForceDeploymentsData, GenesisUpgrade, ZKChainSpecificForceDeploymentsData,
        genesisUpgradeCall, upgradeCall,
    },
};

#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct GenesisUpgradeLocalInfo {
    /// Bytecodes that were force-deployed as part of the genesis upgrade.
    pub force_deploy_info: BTreeMap<Address, BytecodeInfo>,
}

/// Scan over blocks from 'from' to 'to' (inclusive) looking for GenesisUpgrade event.
pub async fn get_genesis_upgrade<P: Provider + Clone>(
    provider: &P,
    diamond_proxy_address: Address,
    from: u64,
    to: u64,
    chunk: u64,
    concurrency: usize,
) -> Result<GenesisUpgradeLocalInfo> {
    let chunks_total = (to - from) / chunk + 1;

    tracing::info!(
        "Looking for GenesisUpgrade event: scanning {} chunks (concurrency={})",
        chunks_total,
        concurrency
    );

    use futures::stream::{self, StreamExt};

    let chunk_ranges: Vec<_> = (0..chunks_total)
        .map(|i| {
            let start = from + (i * chunk);
            let end = (start + chunk - 1).min(to);
            (start, end)
        })
        .collect();

    let mut stream = stream::iter(chunk_ranges)
        .map(|(start, end)| {
            let provider = provider.clone();
            async move { scan_genesis_upgrade(&provider, diamond_proxy_address, start, end).await }
        })
        .buffer_unordered(concurrency);

    let mut chunks_processed = 0;
    while let Some(result) = stream.next().await {
        chunks_processed += 1;
        if chunks_processed % concurrency == 0 {
            tracing::info!(
                "Genesis scan progress: {}/{} chunks ({:.1}%)",
                chunks_processed,
                chunks_total,
                (chunks_processed as f64 / chunks_total as f64) * 100.0
            );
        }

        if let Ok(Some(genesis)) = result {
            tracing::info!(
                "Found GenesisUpgrade event after scanning {} chunks",
                chunks_processed
            );
            return Ok(genesis);
        }
    }

    anyhow::bail!("No GenesisUpgrade event found in the specified range")
}

async fn scan_genesis_upgrade<P: Provider + Clone>(
    provider: &P,
    address: Address,
    from: u64,
    to: u64,
) -> Result<Option<GenesisUpgradeLocalInfo>> {
    let filter = Filter::new()
        .address(address)
        .event_signature(GenesisUpgrade::SIGNATURE_HASH)
        .from_block(from)
        .to_block(to);

    // Fetch all matching logs.
    let logs = provider
        .get_logs(&filter)
        .await
        .context("get_logs(NewPriorityRequest)")?;

    let mut results = None;

    for lg in logs {
        // Decode the event (topics+data) using the generated type.
        // Convert the RPC log into the primitives Log expected by the SolEvent decoder.
        let prim_log = alloy::primitives::Log {
            address: lg.address(),
            data: lg.data().clone(), /*data: alloy::primitives::LogData:::new_unchecked(
                                         lg.topics().clone().into(),
                                         lg.data().clone(),
                                     ),*/
        };
        if let Ok(ev) = GenesisUpgrade::decode_log(&prim_log) {
            tracing::debug!(
                "GenesisUpgrade: zkChain=0x{} protocolVersion={} txType={} from=0x{} to=0x{} nonce={} value={} data_len={} factoryDeps={} internal_deps={}",
                hex::encode(ev._zkChain.as_slice()),
                ev._protocolVersion,
                ev._l2Transaction.txType,
                hex::encode(ev._l2Transaction.from.to_be_bytes_vec()),
                hex::encode(ev._l2Transaction.to.to_be_bytes_vec()),
                ev._l2Transaction.nonce,
                ev._l2Transaction.value,
                ev._l2Transaction.data.len(),
                ev._l2Transaction.factoryDeps.len(),
                ev._factoryDeps.len(),
            );

            let mut factory_deps_analyzed = BTreeMap::new();

            for dep in ev._factoryDeps.iter() {
                let analysis_result = analyze_bytecode(&dep);
                factory_deps_analyzed.insert(analysis_result.bytecode_hash, analysis_result);
            }

            // Now a bunch of hacks, to decode the actual L2 tx and preimages.
            let upgrade = upgradeCall::abi_decode(&ev._l2Transaction.data).unwrap();
            tracing::debug!(
                "updated calldata len: {} full: {}",
                upgrade._calldata.len(),
                hex::encode(&upgrade._calldata)
            );

            let genesis_upgrade = genesisUpgradeCall::abi_decode(&upgrade._calldata).unwrap();

            tracing::debug!(
                "genesisUpgrade: isZKsyncOS={} chainId={} ctmDeployer=0x{} fixedForceDeploymentsData_len={} additionalForceDeploymentsData_len={}",
                genesis_upgrade._isZKsyncOS,
                genesis_upgrade._chainId,
                hex::encode(genesis_upgrade._ctmDeployer.as_slice()),
                genesis_upgrade._fixedForceDeploymentsData.len(),
                genesis_upgrade._additionalForceDeploymentsData.len(),
            );

            let fixed_deployment_data =
                FixedForceDeploymentsData::abi_decode(&genesis_upgrade._fixedForceDeploymentsData)
                    .unwrap();

            tracing::debug!("Fixed deployment full info {:#?}", fixed_deployment_data);

            let zkchain_deployment_data = ZKChainSpecificForceDeploymentsData::abi_decode(
                &genesis_upgrade._additionalForceDeploymentsData,
            )
            .unwrap();

            tracing::debug!(
                "ZKChain-specific deployment full info {:#?}",
                zkchain_deployment_data
            );

            // and now the hacky part begins.

            let mut result = GenesisUpgradeLocalInfo::default();

            let bridgehub_info = BytecodeInfo::parse(
                &fixed_deployment_data.bridgehubBytecodeInfo,
                &factory_deps_analyzed,
            );
            let bridgehub_address = address!("0000000000000000000000000000000000010002");

            result
                .force_deploy_info
                .insert(bridgehub_address, bridgehub_info);

            let l2_asset_router_info = BytecodeInfo::parse(
                &fixed_deployment_data.l2AssetRouterBytecodeInfo,
                &factory_deps_analyzed,
            );
            let l2_asset_router_address = address!("0000000000000000000000000000000000010003");

            result
                .force_deploy_info
                .insert(l2_asset_router_address, l2_asset_router_info);

            let l2_ntv_info = BytecodeInfo::parse(
                &fixed_deployment_data.l2NtvBytecodeInfo,
                &factory_deps_analyzed,
            );
            let l2_ntv_address = address!("0000000000000000000000000000000000010004");
            result.force_deploy_info.insert(l2_ntv_address, l2_ntv_info);

            let message_root_info = BytecodeInfo::parse(
                &fixed_deployment_data.messageRootBytecodeInfo,
                &factory_deps_analyzed,
            );
            let message_root_address = address!("0000000000000000000000000000000000010005");
            result
                .force_deploy_info
                .insert(message_root_address, message_root_info);

            let chain_asset_handler_info = BytecodeInfo::parse(
                &fixed_deployment_data.chainAssetHandlerBytecodeInfo,
                &factory_deps_analyzed,
            );
            let chain_asset_handler_address = address!("000000000000000000000000000000000001000a");
            result
                .force_deploy_info
                .insert(chain_asset_handler_address, chain_asset_handler_info);

            let beacon_deployer_info = BytecodeInfo::parse(
                &fixed_deployment_data.beaconDeployerInfo,
                &factory_deps_analyzed,
            );
            let beacon_deployer_address = address!("000000000000000000000000000000000001000b");
            result
                .force_deploy_info
                .insert(beacon_deployer_address, beacon_deployer_info);

            results = Some(result);
        }
    }

    Ok(results)
}
