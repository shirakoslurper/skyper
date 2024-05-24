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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// const JSON_RPC_URL: &str = "https://mainnet.sanko.xyz";
const WS_RPC_URL: &str = "wss://mainnet.sanko.xyz/ws";
const CAMELOT_POOL_FACTORY_ADDRESS: &str = "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f";
const WDMT_ADDRESS: &str = "0x754cDAd6f5821077d6915004Be2cE05f93d176f8";

abigen!(
    UniswapV2Pair,
    r#"[
        event Approval(address indexed owner, address indexed spender, uint value)
        event Transfer(address indexed from, address indexed to, uint value)
    
        function name() external pure returns (string memory)
        function symbol() external pure returns (string memory)
        function decimals() external pure returns (uint8)
        function totalSupply() external view returns (uint)
        function balanceOf(address owner) external view returns (uint)
        function allowance(address owner, address spender) external view returns (uint)
    
        function approve(address spender, uint value) external returns (bool)
        function transfer(address to, uint value) external returns (bool)
        function transferFrom(address from, address to, uint value) external returns (bool)
    
        function DOMAIN_SEPARATOR() external view returns (bytes32)
        function PERMIT_TYPEHASH() external pure returns (bytes32)
        function nonces(address owner) external view returns (uint)
    
        function permit(address owner, address spender, uint value, uint deadline, uint8 v, bytes32 r, bytes32 s) external
    
        event Mint(address indexed sender, uint amount0, uint amount1)
        event Burn(address indexed sender, uint amount0, uint amount1, address indexed to)
        event Swap(address indexed sender, uint amount0In, uint amount1In, uint amount0Out, uint amount1Out, address indexed to)
        event Sync(uint112 reserve0, uint112 reserve1)
    
        function MINIMUM_LIQUIDITY() external pure returns (uint)
        function factory() external view returns (address)
        function token0() external view returns (address)
        function token1() external view returns (address)
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)
        function price0CumulativeLast() external view returns (uint)
        function price1CumulativeLast() external view returns (uint)
        function kLast() external view returns (uint)
    
        function mint(address to) external returns (uint liquidity)
        function burn(address to) external returns (uint amount0, uint amount1)
        function swap(uint amount0Out, uint amount1Out, address to, bytes calldata data) external
        function skim(address to) external
        function sync() external
    
        function initialize(address, address) external
    ]"#
);

abigen!(
    ERC20Contract,
    r#"[
        event Transfer(address indexed from, address indexed to, uint256 value)
        event Approval(address indexed owner, address indexed spender, uint256 value)
        function totalSupply() external view returns (uint256)
        function balanceOf(address account) external view returns (uint256)
        function transfer(address to, uint256 amount) external returns (bool)
        function allowance(address owner, address spender) external view returns (uint256)
        function approve(address spender, uint256 value) external returns (bool)
        function transferFrom(address from, address to, uint256 value) external returns (bool)
        function decimals() external view returns (uint8)
        function symbol() external view returns (string memory)
    ]"#,
);

// EXECUTION
abigen!(
    UniswapV2Router02,
    r#"[
        function quote(uint amountA, uint reserveA, uint reserveB) external pure returns (uint amountB)
        function getAmountOut(uint amountIn, uint reserveIn, uint reserveOut) external pure returns (uint amountOut)
        function getAmountIn(uint amountOut, uint reserveIn, uint reserveOut) external pure returns (uint amountIn)
        function getAmountsOut(uint amountIn, address[] calldata path) external view returns (uint[] memory amounts)
        function getAmountsIn(uint amountOut, address[] calldata path) external view returns (uint[] memory amounts)
        function removeLiquidityETHSupportingFeeOnTransferTokens(address token, uint liquidity, uint amountTokenMin, uint amountETHMin, address to, uint deadline) external returns (uint amountETH)
        function removeLiquidityETHWithPermitSupportingFeeOnTransferTokens(address token, uint liquidity, uint amountTokenMin, uint amountETHMin, address to, uint deadline, bool approveMax, uint8 v, bytes32 r, bytes32 s) external returns (uint amountETH)
        function swapExactTokensForTokensSupportingFeeOnTransferTokens(uint amountIn, uint amountOutMin, address[] calldata path, address to, uint deadline) external
        function swapExactETHForTokensSupportingFeeOnTransferTokens(uint amountOutMin, address[] calldata path, address to, uint deadline) external payable
        function swapExactTokensForETHSupportingFeeOnTransferTokens(uint amountIn, uint amountOutMin, address[] calldata path, address to, uint deadline) external
    ]"#
);

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
    #[clap(short = 'p', long, default_value = "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f")]
    uni_v2_pool_factory_address: String,
    #[clap(short = 'r', long, default_value = "0x18E621B64d7808c3C47bccbbD7485d23F257D26f")]
    uni_v2_router_address: String,
    #[clap(short = 'b', long, default_value = "0x754cDAd6f5821077d6915004Be2cE05f93d176f8")]
    wrapped_base_coin_address: String,
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
    let provider = Arc::new(Provider::<Ws>::connect(WS_RPC_URL).await?);

    // // CONNECT WALLET TO PROVIDER
    let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.with_chain_id(config.chain_id)));

    // QUICK BLOCK NUMBER CHECK
    let block_number: U64 = provider.get_block_number().await?;
    println!("{block_number}");

    // BASE COIN DETAILS
    let wrapped_base_coin_address = config.wrapped_base_coin_address.parse::<Address>()?;
    let wrapped_base_coin_contract = ERC20Contract::new(wrapped_base_coin_address, client.clone());
    let wrapped_base_coin_decimals = wrapped_base_coin_contract.decimals().call().await?;
    // let wrapped_base_coin_symbol = wrapped_base_coin_contract.symbol().call().await?;
    
    // UNI V2 ROUTER DETAILS
    let uni_v2_router_address = config.uni_v2_router_address.parse::<Address>()?;
    let uni_v2_router_contract = UniswapV2Router02::new(uni_v2_router_address, client.clone());

    // PAIRCREATED AND MINT FILTERS
    let token_topics = [
        H256::from(wrapped_base_coin_address)
    ];

    let pair_created_filter = Filter::new()
        .address(config.uni_v2_pool_factory_address.parse::<Address>()?)
        .event("PairCreated(address,address,adress,uint256)")
        .topic1(token_topics.to_vec())
        .topic2(token_topics.to_vec());

    let mint_filter = Filter::new()
        .event("Mint(address,uint256,uint256)");

    // PAIRCREATED AND MINT EVENT STREAMS
    let pair_created_stream = provider
        .subscribe_logs(&pair_created_filter)
        .await?
        .map(Box::new(|log| EventType::PairCreated(log)) as Box<dyn Fn(Log) -> EventType>);
    let mint_stream = provider
        .subscribe_logs(&mint_filter)
        .await?
        .map(Box::new(|log| EventType::Mint(log)) as Box<dyn Fn(Log) -> EventType>);

    let mut combined_stream = stream::select_all(vec![
        pair_created_stream,
        mint_stream,
    ]);

    // PAIR SET (REMOVED UPON FIRST LIQUIDITY)
    let mut pair_address_set = HashSet::new();


    // VALIDATION
    let wallet_base_coin_balance = client
        .get_balance(client.address(), None)
        .await?;

    println!("wallet_base_coin_balance: {}", wallet_base_coin_balance);

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

                println!("Mint:\n    pair_address: {}", pair_address);

                // if pair_address_set.remove(&pair_address) {

                    // PARSE MINT AMOUNTS IN
                    let sender_address = Address::from(log.topics[1]);
    
                    let amount_0 = U256::from_big_endian(&log.data[0..32]);
                    let amount_1 = U256::from_big_endian(&log.data[32..64]);
    
                    println!("    sender: {}\n    amount_0: {}\n    amount_1: {}", sender_address, amount_0, amount_1);
                    
                    // Buy if it meets liquidity criteria!
                    // Find which amount is the base coin
                    
                    // PAIR CONTRACT
                    let pair_contract = UniswapV2Pair::new(pair_address, client.clone());

                    let token_0 = Address::from(pair_contract.token_0().call().await?);
                    println!("token0: {:?}", token_0);

                    let token_1 = Address::from(pair_contract.token_1().call().await?);
                    println!("token1: {:?}", token_1);

                    // We're subscribed to all Mint events so neither 
                    // token is guaranteed to be our base coin
                    // If/else over match for simplicity (no new scope)
                    let (base_coin_amount, other_coin_address) = if wrapped_base_coin_address == token_0 {
                        (amount_0, token_1)
                    } else if wrapped_base_coin_address == token_1 {
                        (amount_1, token_0)
                    } else {
                        continue;
                    };
                    
                    // CHECK MINIMUM AMOUNT OF BASE COIN RESERVES
                    // if base_coin_amount > U256::from(10) * base_coin_decimals {

                        // BET SIZING
                        // let wallet_base_coin_balance = base_coin_contract
                        // .balance_of(client.address())
                        // .call()
                        // .await?;

                        // println!("wallet_base_coin_balance: {}", wallet_base_coin_balance);

                        let wallet_base_coin_balance = client
                        .get_balance(client.address(), None)
                        .await?;
                
                        println!("wallet_base_coin_balance: {}", wallet_base_coin_balance);

                        let base_coin_amount_in = wallet_base_coin_balance / 20;

                        // BASE COIN DOES NOT NEED TO BE APPROVED
                        // // APPROVE ROUTER TO USE BASE COIN
                        // let approve_receipt = base_coin_contract
                        //     .approve(uni_v2_router_address, base_coin_amount_in)
                        //     .send()
                        //     .await?
                        //     .await?
                        //     .expect("approve() failed: no receipt found");

                        // println!("Router {} approved to trade for {}!\nreceipt: {:?}", uni_v2_router_address, other_coin_address, approve_receipt);

                        // SWAP THROUGH ROUTER
                        let amounts_out = uni_v2_router_contract
                            .get_amounts_out(
                                base_coin_amount_in,
                                vec![wrapped_base_coin_address, other_coin_address]
                            )
                            .call()
                            .await?;

                        println!("{:?}", amounts_out);

                        let deadline = U256::from(get_epoch_milliseconds()) + U256::from(60 * 1000);

                        println!("{:?}", deadline);

                        let swap_receipt = uni_v2_router_contract
                            .swap_exact_eth_for_tokens_supporting_fee_on_transfer_tokens(
                                amounts_out[1]/2, 
                                vec![wrapped_base_coin_address, other_coin_address], 
                                client.address(), 
                                deadline
                            )
                            .value(base_coin_amount_in)
                            .from(client.address())
                            .gas(U256::from(1_000_000))
                            .send()
                            .await?
                            .await?
                            .expect("swapExactEthForTokensSupportingFeeOnTransferTokens() failed: no receipt found");

                        println!("Router {} swapped for {}!\nreceipt: {:?}", uni_v2_router_address, other_coin_address, swap_receipt);
                    // }

                // }
            }
        }
    }

    Ok(())
}

fn get_epoch_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}