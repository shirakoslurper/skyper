use ethers::prelude::*;
use futures::{stream, StreamExt, TryStreamExt};
use std::collections::HashSet;


// const JSON_RPC_URL: &str = "https://mainnet.sanko.xyz";
const WS_RPC_URL: &str = "wss://mainnet.sanko.xyz/ws";
const CAMELOT_POOL_FACTORY_ADDRESS: &str = "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f";
const WDMT_ADDRESS: &str = "0x754cDAd6f5821077d6915004Be2cE05f93d176f8";

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let provider = Provider::<Ws>::connect(WS_RPC_URL).await?;

    let block_number: U64 = provider.get_block_number().await?;
    println!("{block_number}");

    let camelot_pool_factory_address = CAMELOT_POOL_FACTORY_ADDRESS.parse::<H160>()?;

    let token_topics = [
        H256::from(WDMT_ADDRESS.parse::<Address>()?)
    ];

    let pair_created_filter = Filter::new()
        .address(CAMELOT_POOL_FACTORY_ADDRESS.parse::<Address>()?)
        .event("PairCreated(address,address,adress,uint256)")
        .topic1(token_topics.to_vec())
        .topic2(token_topics.to_vec());

    let mint_filter = Filter::new()
        .event("Mint(address,uint256,uint256)");

    let pair_created_stream = provider.subscribe_logs(&pair_created_filter).await?;
    let mint_stream = provider.subscribe_logs(&mint_filter).await?;

    // We may want to skip combining and have these two in different processes as
    // it's a bit of a pain to tell these events apart
    // Simplest hack would be the contract address tho

    let mut combined_stream = stream::select_all(vec![
        pair_created_stream,
        mint_stream,
    ]);

    let mut pair_address_set = HashSet::new();

    while let Some(log) = combined_stream.next().await {
        println!("{:#?}", log);
        if camelot_pool_factory_address == log.address {
            let pair_address = Address::from(&log.data[40..60].try_into()?);
            println!("PairCreated:\n    pair_address: {}", pair_address);
            pair_address_set.insert(pair_address);
        } else {
            let pair_address = log.address;

            println!("Mint\n    pair_address: {}", pair_address);

            let sender_address = Address::from(log.topics[1]);

            let amount_0 = U256::from_big_endian(&log.data[0..32]);
            let amount_1 = U256::from_big_endian(&log.data[32..64]);

            println!("    sender: {}\n    amount_0: {}\n    amount_1: {}", sender_address, amount_0, amount_1);

            // if pair_address_set.remove(&pair_address) {
            //     let sender_address = Address::from(log.topics[1]);

            //     let amount_0 = U256::from_big_endian(&log.data[0..32]);
            //     let amount_1 = U256::from_big_endian(&log.data[32..64]);

            //     println!("    sender: {}\n    amount_0: {}\n    amount_1: {}", sender_address, amount_0, amount_1);


            //     // Buy if it meets liquidity criteria!
            // }
        }
    

    }




    // let swap_filter = Filter::new()
    //     .event("Swap(address,uint256,uint256,uint256,uint256,address)");
    //     // .topic1(token_topics.to_vec())
    //     // .topic2(token_topics.to_vec());

    // let mut swap_stream = provider.subscribe_logs(&swap_filter).await?;
    // while let Some(log) = swap_stream.next().await {
    //     println!("{:#?}", log);
    // }

    Ok(())
}
