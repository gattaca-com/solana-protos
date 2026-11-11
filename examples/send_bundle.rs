//! Prints the tip accounts and next connected leaders, then sends the given
//! signed transactions (one per file, raw or base64) as one bundle.
//!
//! ```sh
//! cargo run --example send_bundle -- https://sol-ams.titanbuilder.xyz [<tx file>...]
//! ```

use std::{env, error::Error, fs};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use sol_protos::{
    bundle::Bundle,
    packet::Packet,
    searcher::{
        ConnectedLeadersRequest, GetTipAccountsRequest, SendBundleRequest,
        searcher_service_client::SearcherServiceClient,
    },
};
use tonic::transport::{Channel, ClientTlsConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let url = args
        .next()
        .ok_or("usage: send_bundle <url> [<tx file>...]")?;
    let channel = Channel::from_shared(url)?
        .tls_config(ClientTlsConfig::new().with_enabled_roots())?
        .connect()
        .await?;
    let mut searcher = SearcherServiceClient::new(channel);

    let tips = searcher
        .get_tip_accounts(GetTipAccountsRequest {})
        .await?
        .into_inner()
        .accounts;
    println!("tip accounts: {}", tips.join(" "));

    let leaders = searcher
        .get_connected_leaders(ConnectedLeadersRequest {})
        .await?
        .into_inner()
        .connected_validators;
    let mut next: Vec<_> = leaders
        .iter()
        .filter_map(|(identity, slots)| Some((*slots.slots.first()?, identity)))
        .collect();
    next.sort_unstable();
    for (slot, identity) in next.iter().take(5) {
        println!("leader {slot} {identity}");
    }

    let packets = args
        .map(|path| {
            let bytes = fs::read(&path)?;
            let data = std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| BASE64.decode(text.trim()).ok())
                .unwrap_or(bytes);
            Ok(Packet { data, meta: None })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    if packets.is_empty() {
        return Ok(());
    }
    let bundle = Bundle {
        packets,
        ..Default::default()
    };
    let id = searcher
        .send_bundle(SendBundleRequest {
            bundle: Some(bundle),
        })
        .await?
        .into_inner()
        .uuid;
    println!("bundle sent: {id}");
    Ok(())
}
