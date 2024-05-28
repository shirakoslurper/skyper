use clap::Parser;
use ethers::core::k256::ecdsa::SigningKey;
use ethers::core::k256::Secp256k1;
use ethers::prelude::*;
use ethers::{
    core::{types::TransactionRequest, utils::Anvil},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{coins_bip39::English, Signer},
};
use futures::{stream, StreamExt};
use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const CAMELOT_POOL_FACTORY_ADDRESS: &str = "0x7d8c6B58BA2d40FC6E34C25f9A488067Fe0D2dB4";
const CAMELOT_ROUTER_ADDRESS: &str = "0x18E621B64d7808c3C47bccbbD7485d23F257D26f";
const WDMT_ADDRESS: &str = "0x754cDAd6f5821077d6915004Be2cE05f93d176f8";
const SANKO_CHAIN_ID: u64 = 1996;
const SANKO_WS_RPC_URL: &str = "wss://mainnet.sanko.xyz/ws";

abigen!(
    CamelotPair,
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

abigen!(
    CamelotRouter,
    r#"[
        function quote(uint amountA, uint reserveA, uint reserveB) external pure returns (uint amountB)
        function getAmountOut(uint amountIn, uint reserveIn, uint reserveOut) external pure returns (uint amountOut)
        function getAmountIn(uint amountOut, uint reserveIn, uint reserveOut) external pure returns (uint amountIn)
        function getAmountsOut(uint amountIn, address[] calldata path) external view returns (uint[] memory amounts)
        function getAmountsIn(uint amountOut, address[] calldata path) external view returns (uint[] memory amounts)
        function removeLiquidityETHSupportingFeeOnTransferTokens(address token, uint liquidity, uint amountTokenMin, uint amountETHMin, address to, uint deadline) external returns (uint amountETH)
        function removeLiquidityETHWithPermitSupportingFeeOnTransferTokens(address token, uint liquidity, uint amountTokenMin, uint amountETHMin, address to, uint deadline, bool approveMax, uint8 v, bytes32 r, bytes32 s) external returns (uint amountETH)
        function swapExactTokensForTokensSupportingFeeOnTransferTokens(uint amountIn, uint amountOutMin, address[] calldata path, address to, address referrer, uint deadline) external
        function swapExactETHForTokensSupportingFeeOnTransferTokens(uint amountOutMin, address[] calldata path, address to, address referrer, uint deadline) external payable
        function swapExactTokensForETHSupportingFeeOnTransferTokens(uint amountIn, uint amountOutMin, address[] calldata path, address to, address referrer, uint deadline) external
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
    let provider = Arc::new(Provider::<Ws>::connect(config.ws_rpc_url).await?);

    // // CONNECT WALLET TO PROVIDER
    let client = Arc::new(SignerMiddleware::new(provider.clone(), wallet.with_chain_id(SANKO_CHAIN_ID)));

    // QUICK BLOCK NUMBER CHECK
    let block_number: U64 = provider.get_block_number().await?;
    println!("{block_number}");

    // BASE COIN DETAILS
    let wdmt_address = WDMT_ADDRESS.parse::<Address>()?;
    let wdmt_contract = ERC20Contract::new(wdmt_address, client.clone());
    let wdmt_decimals = wdmt_contract.decimals().call().await?;
    // let wrapped_base_coin_symbol = wrapped_base_coin_contract.symbol().call().await?;
    
    // CAMELOT ROUTER DETAILS
    let camelot_router_address = CAMELOT_ROUTER_ADDRESS.parse::<Address>()?;
    let camelot_router_contract = CamelotRouter::new(camelot_router_address, client.clone());

    // PAIRCREATED AND MINT FILTERS
    // let token_topics = [
    //     H256::from(wdmt_address)
    // ];

    let pair_created_filter = Filter::new()
        .address(CAMELOT_POOL_FACTORY_ADDRESS.parse::<Address>()?)
        .event("PairCreated(address,address,address,uint256)");
        // .topic1(token_topics.to_vec())
        // .topic2(token_topics.to_vec());

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

    // VALIDATION AND CHECK
    let wallet_dmt_balance = client
        .get_balance(client.address(), None)
        .await?;

    println!("wallet_dmt_balance: {}", wallet_dmt_balance);

    // MIN DMT RESERVES
    let min_dmt_reserve = U256::from(5) * U256::from(10).pow(U256::from(wdmt_decimals));

    let mut pair_created_pair_address_set = HashSet::new();
    let mut pair_address_to_mint_details = HashMap::new();

    // EVENT HANDLING LOOP
    while let Some(event) = combined_stream.next().await {
        println!("{:#?}", event);

        match event {
            EventType::PairCreated(log) => {
                // 12 empty bytes at start
                let pair_address = Address::from(&log.data[12..32].try_into()?);
                
                println!("PairCreated:\n    pair_address: {}", pair_address);

                pair_address_set.insert(pair_address);

                if let Some(mint_details) = pair_address_to_mint_details.remove(&pair_address) {
                    // MAKE_TRADE WITH MINT INFO THAT SNUCK IN AHEAD
                    trade_dmt_for_other(
                        &client, 
                        wdmt_address,
                        wdmt_decimals,
                        &camelot_router_contract,
                        min_dmt_reserve,
                        pair_address, 
                        mint_details
                    ).await?;
                } else {
                    // PLACE IN pair_created_pair_address_set FOR
                    // FOLLOWING MINT TO REFER TO
                    pair_created_pair_address_set.insert(pair_address);
                }
            },
            EventType::Mint(log) => {
                let pair_address = log.address;

                println!("Mint:\n    pair_address: {}", pair_address);

                let amount_0 = U256::from_big_endian(&log.data[0..32]);
                let amount_1 = U256::from_big_endian(&log.data[32..64]);

                let mint_details = MintDetails {
                    amount_0,
                    amount_1
                };

                if pair_created_pair_address_set.remove(&pair_address) {
                    // EXECUTE TRADE
                    trade_dmt_for_other(
                        &client, 
                        wdmt_address,
                        wdmt_decimals,
                        &camelot_router_contract,
                        min_dmt_reserve,
                        pair_address, 
                        mint_details
                    ).await?;
                } else {
                    // DEFER DETAILS FOR POSSIBLE LATER EXECUTION
                    // MINT MAY SNEAK IN BEFORE PAIR CREATE (TWO STREAMS)
                    pair_address_to_mint_details.insert(
                        pair_address,
                        mint_details
                    ); 
                }
            }
        }
    }

    Ok(())
}

struct MintDetails {
    pub amount_0: U256,
    pub amount_1: U256
}

async fn trade_dmt_for_other(
    client: &Arc<SignerMiddleware<Arc<Provider<Ws>>, Wallet<SigningKey>>>,
    wdmt_address: Address,
    wdmt_decimals: u8,
    camelot_router_contract: &CamelotRouter<SignerMiddleware<Arc<Provider<Ws>>, Wallet<SigningKey>>>,
    min_dmt_reserve: U256,
    pair_address: Address,
    mint_details: MintDetails
) -> eyre::Result<()> {
    // PAIR CONTRACT
    let pair_contract = CamelotPair::new(pair_address, client.clone());

    let token_0 = Address::from(pair_contract.token_0().call().await?);
    println!("token0: {:?}", token_0);

    let token_1 = Address::from(pair_contract.token_1().call().await?);
    println!("token1: {:?}", token_1);

    // We're subscribed to all Mint events so neither 
    // token is guaranteed to be our base coin
    // If/else over match for simplicity (no new scope)
    let (dmt_amount, other_coin_address) = if wdmt_address == token_0 {
        (mint_details.amount_0, token_1)
    } else if wdmt_address == token_1 {
        (mint_details.amount_1, token_0)
    } else {
        return Ok(());
    };
    
    // CHECK MINIMUM AMOUNT OF BASE COIN RESERVES

    if dmt_amount > min_dmt_reserve {

        // BET SIZING
        let wallet_dmt_balance = client
            .get_balance(client.address(), None)
            .await?;

        println!("wallet_base_coin_balance: {}", wallet_dmt_balance);

        let dmt_amount_in = std::cmp::min(
            U256::from(2) * U256::from(10).pow(U256::from(wdmt_decimals)),
            wallet_dmt_balance / 10
        );

        // SWAP THROUGH ROUTER
        let amounts_out = camelot_router_contract
            .get_amounts_out(
                dmt_amount_in,
                vec![wdmt_address, other_coin_address]
            )
            .call()
            .await?;

        println!("amounts_out: {:?}", amounts_out);

        let deadline = U256::from(get_epoch_milliseconds()) + U256::from(60 * 1000);

        println!("deadline: {:?}", deadline);

        let swap_receipt = camelot_router_contract
            .swap_exact_eth_for_tokens_supporting_fee_on_transfer_tokens(
                amounts_out[1]/2, 
                vec![wdmt_address, other_coin_address], 
                client.address(), 
                client.address(),
                deadline
            )
            .value(dmt_amount_in)
            .from(client.address())
            .gas(U256::from(500_000))
            .send()
            .await?
            .await?
            .expect("swapExactEthForTokensSupportingFeeOnTransferTokens() failed: no receipt found");

        println!("Swapped for {}!\nreceipt: {:?}", other_coin_address, swap_receipt);

    }

    Ok(())
}

fn get_epoch_milliseconds() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}