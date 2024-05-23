use clap::Parser;
use ethers::prelude::*;
use ethers::{
    core::{types::TransactionRequest, utils::Anvil},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{coins_bip39::English, Signer},
};
use futures::{stream, Stream, StreamExt, TryStream, TryStreamExt};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

// const JSON_RPC_URL: &str = "https://mainnet.sanko.xyz";
const WS_RPC_URL: &str = "wss://mainnet.sanko.xyz/ws";
const CAMELOT_POOL_FACTORY_ADDRESS: &str = "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f";
const WDMT_ADDRESS: &str = "0x754cDAd6f5821077d6915004Be2cE05f93d176f8";

#[derive(Clone, Debug)]
enum EventType {
    PairCreated(Log),
    Mint(Log)
}

#[derive(Parser, Debug)]
struct Config {
    #[clap(short = 'm', long)]
    mnemonic_dir: PathBuf,
    #[clap(short = 'w', long, default_value = "wss://mainnet.sanko.xyz/ws")]
    ws_rpc_url: String,
    #[clap(short = 'u', long, default_value = "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f")]
    uni_v2_pool_factory_address: String,
    #[clap(short = 'b', long, default_value = "0x754cDAd6f5821077d6915004Be2cE05f93d176f8")]
    base_coin_address: String,
    #[clap(short = 'c', long, default_value = "1996")]
    chain_id: u64
}


#[tokio::main]
async fn main() -> eyre::Result<()> {

    let config = Config::parse();

    // WALLET
    let wallet = MnemonicBuilder::<English>::default()
        .phrase(config.mnemonic_dir)
        .index(0u32)?
        .build()?;
    println!("wallet: {:?}", wallet);


    // CONNECT TO NETWORKS
    let provider = Provider::<Ws>::connect(WS_RPC_URL).await?;

    // // CONNECT WALLET TO PROVIDER
    let client = SignerMiddleware::new(provider.clone(), wallet.with_chain_id(config.chain_id));

    // QUICK BLOCK NUMBER CHECK
    let block_number: U64 = provider.get_block_number().await?;
    println!("{block_number}");

    // PAIRCREATED AND MINT FILTERS
    let token_topics = [
        H256::from(config.base_coin_address.parse::<Address>()?)
    ];

    let pair_created_filter = Filter::new()
        .address(config.uni_v2_pool_factory_address.parse::<Address>()?)
        .event("PairCreated(address,address,adress,uint256)")
        .topic1(token_topics.to_vec())
        .topic2(token_topics.to_vec());

    let mint_filter = Filter::new()
        .event("Mint(address,uint256,uint256)");

    // PAIRCREATED AND MINT EVENT STREAMS
    let pair_created_stream = provider.subscribe_logs(&pair_created_filter).await?.map(Box::new(|log| EventType::PairCreated(log)) as Box<dyn Fn(Log) -> EventType>);
    let mint_stream = provider.subscribe_logs(&mint_filter).await?.map(Box::new(|log| EventType::Mint(log)) as Box<dyn Fn(Log) -> EventType>);

    let mut combined_stream = stream::select_all(vec![
        pair_created_stream,
        mint_stream,
    ]);

    // PAIR SET (REMOVED UPON FIRST LIQUIDITY)
    let mut pair_address_set = HashSet::new();

    // EVENT HANDLING LOOP
    while let Some(event) = combined_stream.next().await {
        println!("{:#?}", event);

        match event {
            EventType::PairCreated(log) => {
                let pair_address = Address::from(&log.data[40..60].try_into()?);
                println!("PairCreated:\n    pair_address: {}", pair_address);
                pair_address_set.insert(pair_address);
            },
            EventType::Mint(log) => {
                let pair_address = log.address;

                // // TODO: Check the pool for the coins
                // // One token *will* be WDMT due to pair creation filtering
                // // Check which one is WDMT and for the one that isn't.
                if pair_address_set.remove(&pair_address) {
                    let sender_address = Address::from(log.topics[1]);
    
                    let amount_0 = U256::from_big_endian(&log.data[0..32]);
                    let amount_1 = U256::from_big_endian(&log.data[32..64]);
    
                    println!("    sender: {}\n    amount_0: {}\n    amount_1: {}", sender_address, amount_0, amount_1);
                    // Buy if it meets liquidity criteria!

                }
            }
        }
    }

    Ok(())
}
