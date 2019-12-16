use crate::clients::directory;
use crate::clients::directory::presence::Topology;
use crate::clients::directory::requests::presence_topology_get::PresenceTopologyGetRequester;
use crate::clients::directory::DirectoryClient;
use crate::clients::mix::MixClient;
use crate::clients::provider::ProviderClient;
use crate::utils::bytes;
use base64;
use clap::ArgMatches;
use curve25519_dalek::montgomery::MontgomeryPoint;
use sphinx::route::Destination;
use sphinx::route::Node as SphinxNode;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::{interval_at, Instant};

pub fn execute(matches: &ArgMatches) {
    let is_local = matches.is_present("local");

    // Grab the network topology from the remote directory server
    let topology = get_topology(is_local);

    // Create the runtime, probably later move it to Client struct itself?
    let mut rt = Runtime::new().unwrap();

    // Spawn the root task
    rt.block_on(async {
        let start = Instant::now() + Duration::from_nanos(1000);
        let mut interval = interval_at(start, Duration::from_millis(1000));
        let mut i: usize = 0;
        loop {
            interval.tick().await;
            let message = format!("Hello, Sphinx {}", i).as_bytes().to_vec();

            let route_len = 2;

            // data needed to generate a new Sphinx packet
            let route = route_from(&topology, route_len);
            let destination = get_destination();
            let delays = sphinx::header::delays::generate(route_len);

            // build the packet
            let packet =
                sphinx::SphinxPacket::new(message, &route[..], &destination, &delays).unwrap();

            // send to mixnet
            let mix_client = MixClient::new();
            let result = mix_client.send(packet, route.first().unwrap()).await;
            println!("packet sent:  {:?}", i);
            i += 1;

            // retrieve messages every now and then
            if i % 3 == 0 {
                interval.tick().await;
                println!("going to retrieve messages!");
                let provider_client = ProviderClient::new();
                provider_client.retrieve_messages().await.unwrap();
            }
        }
    })
}

fn get_topology(is_local: bool) -> Topology {
    let url = if is_local {
        "http://localhost:8080".to_string()
    } else {
        "https://directory.nymtech.net".to_string()
    };
    println!("Using directory server: {:?}", url);
    let directory_config = directory::Config { base_url: url };
    let directory = directory::Client::new(directory_config);

    let topology = directory
        .presence_topology
        .get()
        .expect("Failed to retrieve network topology.");
    topology
}

fn route_from(topology: &Topology, route_len: usize) -> Vec<SphinxNode> {
    let mut route = vec![];
    let nodes = topology.mix_nodes.iter();
    for mix in nodes.take(route_len) {
        let address_bytes = bytes::zero_pad_to_32(mix.host.as_bytes().to_vec());
        let decoded_key_bytes = base64::decode_config(&mix.pub_key, base64::URL_SAFE).unwrap();
        let key_bytes = bytes::zero_pad_to_32(decoded_key_bytes);
        let key = MontgomeryPoint(key_bytes);
        let mut sphinx_node = SphinxNode {
            address: address_bytes,
            pub_key: key,
        };

        // temporary to make it work locally:
        sphinx_node.pub_key = Default::default();
        route.push(sphinx_node);
    }
    route
}

// TODO: where do we retrieve this guy from?
fn get_destination() -> Destination {
    Destination {
        address: [42u8; 32],
        identifier: [1u8; 16],
    }
}
