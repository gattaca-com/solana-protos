//! A complete searcher client: connects to a Titan Solana bundle API
//! endpoint, authenticates if a keypair is given (registered searchers only;
//! unregistered use needs none), prints the regions, tip accounts and connected
//! leaders, and sends the signed transactions given on the command line as
//! one bundle.
//!
//! ```sh
//! cargo run --example send_bundle -- \
//!     --url https://sol-ams.titanbuilder.xyz \
//!     [--keypair ~/.config/solana/searcher.json] \
//!     [--drop <index>]... [--slot <slot>] [<tx file>...]
//! ```
//!
//! Each `<tx file>` holds one signed, serialized transaction, raw or base64.
//! With no transaction files the example only queries the read calls.

use std::{env, error::Error, fs, path::PathBuf, process::ExitCode, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use sol_protos::{
    auth::{
        GenerateAuthChallengeRequest, GenerateAuthTokensRequest, Role,
        auth_service_client::AuthServiceClient,
    },
    bundle::Bundle,
    packet::Packet,
    searcher::{
        ConnectedLeadersRequest, GetRegionsRequest, GetTipAccountsRequest, SendBundleRequest,
        searcher_service_client::SearcherServiceClient,
    },
};
use solana_keypair::{Keypair, read_keypair_file};
use solana_signer::Signer;
use tonic::{
    Request,
    metadata::{Ascii, MetadataValue},
    transport::{Channel, ClientTlsConfig},
};

struct Args {
    url: String,
    keypair: Option<PathBuf>,
    dropping_indices: Vec<u32>,
    slot: Option<u64>,
    txs: Vec<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = env::args().skip(1);
    let mut url = None;
    let mut keypair = None;
    let mut dropping_indices = Vec::new();
    let mut slot = None;
    let mut txs = Vec::new();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--url" => url = Some(value("--url")?),
            "--keypair" => keypair = Some(PathBuf::from(value("--keypair")?)),
            "--drop" => dropping_indices.push(
                value("--drop")?
                    .parse()
                    .map_err(|e| format!("--drop: {e}"))?,
            ),
            "--slot" => {
                slot = Some(
                    value("--slot")?
                        .parse()
                        .map_err(|e| format!("--slot: {e}"))?,
                )
            }
            path => txs.push(PathBuf::from(path)),
        }
    }
    Ok(Args {
        url: url.ok_or("--url is required")?,
        keypair,
        dropping_indices,
        slot,
        txs,
    })
}

/// Reads one transaction file, raw bytes or base64 text.
fn read_tx(path: &PathBuf) -> Result<Vec<u8>, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let text = std::str::from_utf8(&bytes).map(str::trim);
    match text.ok().and_then(|text| BASE64.decode(text).ok()) {
        Some(decoded) => Ok(decoded),
        None => Ok(bytes),
    }
}

/// The Jito auth flow: a challenge signed as `"<pubkey>-<challenge>"`.
async fn authenticate(
    channel: Channel,
    keypair: &Keypair,
) -> Result<MetadataValue<Ascii>, Box<dyn Error>> {
    let mut auth = AuthServiceClient::new(channel);
    let challenge = auth
        .generate_auth_challenge(GenerateAuthChallengeRequest {
            role: Role::Searcher as i32,
            pubkey: keypair.pubkey().to_bytes().to_vec(),
        })
        .await?
        .into_inner()
        .challenge;
    let challenge = format!("{}-{challenge}", keypair.pubkey());
    let signature = keypair.sign_message(challenge.as_bytes());
    let tokens = auth
        .generate_auth_tokens(GenerateAuthTokensRequest {
            challenge,
            client_pubkey: keypair.pubkey().to_bytes().to_vec(),
            signed_challenge: signature.as_ref().to_vec(),
        })
        .await?
        .into_inner();
    let token = tokens
        .access_token
        .ok_or("the API returned no access token")?
        .value;
    Ok(format!("Bearer {token}").parse()?)
}

/// A request with the bearer token, if any, and a deadline.
fn request<T>(message: T, bearer: Option<&MetadataValue<Ascii>>) -> Request<T> {
    let mut request = Request::new(message);
    if let Some(bearer) = bearer {
        request
            .metadata_mut()
            .insert("authorization", bearer.clone());
    }
    request.set_timeout(Duration::from_secs(2));
    request
}

async fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let mut endpoint = Channel::from_shared(args.url.clone())?
        .connect_timeout(Duration::from_secs(5))
        .http2_keep_alive_interval(Duration::from_secs(10))
        .keep_alive_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true);
    if args.url.starts_with("https://") {
        endpoint = endpoint.tls_config(ClientTlsConfig::new().with_enabled_roots())?;
    }
    let channel = endpoint.connect().await?;

    let bearer = match &args.keypair {
        Some(path) => {
            let keypair =
                read_keypair_file(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
            let bearer = authenticate(channel.clone(), &keypair).await?;
            println!("authenticated as {}", keypair.pubkey());
            Some(bearer)
        }
        None => None,
    };
    let bearer = bearer.as_ref();
    let mut searcher = SearcherServiceClient::new(channel);

    let regions = searcher
        .get_regions(request(GetRegionsRequest {}, bearer))
        .await?
        .into_inner();
    println!(
        "region {} of {:?}",
        regions.current_region, regions.available_regions
    );

    let tips = searcher
        .get_tip_accounts(request(GetTipAccountsRequest {}, bearer))
        .await?
        .into_inner()
        .accounts;
    println!("tip accounts: {}", tips.join(" "));

    let leaders = searcher
        .get_connected_leaders(request(ConnectedLeadersRequest {}, bearer))
        .await?
        .into_inner()
        .connected_validators;
    let mut next: Vec<(u64, &String)> = leaders
        .iter()
        .filter_map(|(identity, slots)| slots.slots.first().map(|&slot| (slot, identity)))
        .collect();
    next.sort_unstable();
    println!("{} connected validators; next leader slots:", leaders.len());
    for (slot, identity) in next.iter().take(5) {
        println!("  {slot} {identity}");
    }

    if args.txs.is_empty() {
        return Ok(());
    }
    let packets = args
        .txs
        .iter()
        .map(|path| {
            Ok(Packet {
                data: read_tx(path)?,
                meta: None,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let bundle = Bundle {
        packets,
        dropping_indices: args.dropping_indices,
        slot: args.slot,
        ..Bundle::default()
    };
    let mut request = request(
        SendBundleRequest {
            bundle: Some(bundle),
        },
        bearer,
    );
    request.set_timeout(Duration::from_millis(500));
    match searcher.send_bundle(request).await {
        Ok(response) => println!("bundle accepted: {}", response.into_inner().uuid),
        Err(status) => println!("bundle rejected: {:?} {}", status.code(), status.message()),
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            eprintln!(
                "usage: send_bundle --url <url> [--keypair <file>] [--drop <i>]... [--slot <n>] [<tx file>...]"
            );
            return ExitCode::FAILURE;
        }
    };
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
