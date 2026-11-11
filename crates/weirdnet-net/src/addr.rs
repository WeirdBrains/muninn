//! Addresses: which are worth giving to others, which are reachable from elsewhere, how
//! good each is for reaching a relay, and one circuit per relay.
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};

/// The peer id at the end of an address (`…/p2p/ID`).
pub fn addr_peer(a: &Multiaddr) -> Option<PeerId> {
    a.iter().filter_map(|p| if let Protocol::P2p(id) = p { Some(id) } else { None }).last()
}

/// Parse addresses, skipping bad ones.
pub fn parse_addrs<S: AsRef<str>>(v: &[S]) -> Vec<Multiaddr> {
    v.iter().filter_map(|s| s.as_ref().trim().parse().ok()).collect()
}

/// Whether an address is worth giving to others: not unspecified, not IPv6 link-local
/// (`fe80::`, needs an interface to dial), not a Docker bridge (172.17/16, 172.18/16,
/// unreachable from other hosts) — anywhere in it, so a relay circuit through one is left
/// out too. Loopback stays: nodes on one host use it.
pub fn shareable(a: &Multiaddr) -> bool {
    !a.iter().any(|p| match p {
        Protocol::Ip4(ip) => ip.is_unspecified() || (ip.octets()[0] == 172 && matches!(ip.octets()[1], 17 | 18)),
        Protocol::Ip6(ip) => ip.is_unspecified() || (ip.segments()[0] & 0xffc0) == 0xfe80,
        _ => false,
    })
}

/// Whether hosts elsewhere on the internet can reach `a`: not loopback, private
/// (RFC 1918), CGNAT (100.64/10), link-local, IPv6 ULA (fc00::/7) or unspecified.
/// DNS names count as routable. A relay only announces these.
pub fn routable(a: &Multiaddr) -> bool {
    if !shareable(a) {
        return false;
    }
    !a.iter().any(|p| match p {
        Protocol::Ip4(ip) => {
            let o = ip.octets();
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        Protocol::Ip6(ip) => {
            let s0 = ip.segments()[0];
            ip.is_loopback() || (s0 & 0xfe00) == 0xfc00 || (s0 & 0xffc0) == 0xfe80
        }
        _ => false,
    })
}

/// How good an address is to reach a relay from elsewhere (lower is better): a DNS name,
/// then a public IP, a private LAN one, CGNAT/VPN (100.64/10) or IPv6 ULA, and loopback
/// last; TCP before QUIC at each step (a dead TCP address fails at once).
pub fn reach_rank(a: &Multiaddr) -> u8 {
    let mut r = 1;
    for p in a.iter() {
        r = match p {
            Protocol::Dns(_) | Protocol::Dns4(_) | Protocol::Dns6(_) => 0,
            Protocol::Ip4(ip) if ip.is_loopback() => 5,
            Protocol::Ip4(ip) if ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64 => 3,
            Protocol::Ip4(ip) if ip.is_private() => 2,
            Protocol::Ip4(_) => 1,
            Protocol::Ip6(ip) if ip.is_loopback() => 5,
            Protocol::Ip6(ip) if (ip.segments()[0] & 0xfe00) == 0xfc00 => 4,
            Protocol::Ip6(_) => 1,
            _ => continue,
        };
        break;
    }
    let quic = a.iter().any(|p| matches!(p, Protocol::QuicV1));
    r * 2 + quic as u8
}

/// The relay a circuit address (`…/p2p/RELAY/p2p-circuit[/p2p/PEER]`) goes through.
fn circuit_relay(a: &Multiaddr) -> Option<PeerId> {
    let mut relay = None;
    for p in a.iter() {
        match p {
            Protocol::P2p(id) => relay = Some(id),
            Protocol::P2pCircuit => return relay,
            _ => {}
        }
    }
    None
}

/// Keep one circuit address per relay, the most reachable one (a relay the user named
/// first). libp2p dials the relay once per circuit address and the first of those dials to
/// fail cancels the others, so many addresses for one relay (a relay listens on Docker,
/// VPN and loopback addresses too) make the peer unreachable instead of better reached.
pub fn one_circuit_per_relay(addrs: Vec<Multiaddr>, configured: &[Multiaddr]) -> Vec<Multiaddr> {
    let rank = |a: &Multiaddr| {
        let base: Multiaddr = a.iter().take_while(|p| *p != Protocol::P2pCircuit).collect();
        (!configured.contains(&base), reach_rank(a))
    };
    let mut best: Vec<(PeerId, Multiaddr)> = vec![];
    let mut out = vec![];
    for a in addrs {
        match circuit_relay(&a) {
            None => out.push(a),
            Some(r) => match best.iter_mut().find(|(p, _)| *p == r) {
                Some((_, b)) if rank(&a) < rank(b) => *b = a,
                Some(_) => {}
                None => best.push((r, a)),
            },
        }
    }
    out.extend(best.into_iter().map(|(_, a)| a));
    out
}

#[cfg(test)]
#[test]
fn one_reachable_circuit_per_relay() {
    let r = "12D3KooWC21S2Wxq3UCGcZYTsipGKjRitEar4nFARuvzpiUv2HH2";
    let m = "12D3KooWPNK5evehBHG3pftjw1tN1VuH7CLjo8dZfMxes9tdQHTg";
    let c = |base: &str| -> Multiaddr { format!("{base}/p2p/{r}/p2p-circuit").parse().unwrap() };
    let direct: Multiaddr = "/ip4/192.168.50.230/tcp/4741".parse().unwrap();
    let all = vec![
        direct.clone(),
        c("/ip4/127.0.0.1/tcp/4741"),
        c("/ip4/100.75.141.6/udp/4741/quic-v1"),
        c("/ip4/192.168.50.2/tcp/4741"),
        c("/dns4/alien.example/udp/4741/quic-v1"),
        c("/dns4/alien.example/tcp/4741"),
    ];
    assert_eq!(one_circuit_per_relay(all.clone(), &[]), vec![direct.clone(), c("/dns4/alien.example/tcp/4741")]);
    let named: Multiaddr = format!("/ip4/192.168.50.2/tcp/4741/p2p/{r}").parse().unwrap();
    assert_eq!(one_circuit_per_relay(all, &[named]), vec![direct, c("/ip4/192.168.50.2/tcp/4741")]);
    let docker: Multiaddr = format!("/ip4/172.17.0.1/tcp/4741/p2p/{r}/p2p-circuit/p2p/{m}").parse().unwrap();
    let fe80: Multiaddr = format!("/ip6/fe80::1/tcp/4741/p2p/{r}/p2p-circuit/p2p/{m}").parse().unwrap();
    assert!(!shareable(&docker) && !shareable(&fe80));
    assert!(shareable(&"/ip4/127.0.0.1/tcp/1".parse().unwrap()) && shareable(&"/ip4/172.16.0.1/tcp/1".parse().unwrap()));
}
