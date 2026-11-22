//! 本机网络类型的轻量探测。
//!
//! “仅局域网可用”的线路（家里那台 NAS）在桌面端等价于手机上的“仅 Wi-Fi”：
//! 只要本机存在一条已启用、非回环的局域网地址，就认为可以用这类线路。
//! 探测失败时返回 `true`：宁可多试一条线路，也不要凭一次失败把所有线路都跳过。

/// 本机是否接入了局域网 / Wi-Fi（存在非回环的私有或链路本地地址）。
///
/// 回环、容器网桥（`172.17.0.0/16`）、VPN 隧道都算不上“在家里”，因此只认
/// `10/8`、`172.16/12`、`192.168/16` 与 IPv6 的 ULA / 链路本地地址。
pub fn on_local_network() -> bool {
    let addresses = match if_addrs::get_if_addrs() {
        Ok(addresses) => addresses,
        Err(error) => {
            tracing::debug!(%error, "无法枚举本机网卡，按“在局域网”处理");
            return true;
        }
    };
    addresses.iter().any(|interface| is_local_address(&interface.ip()))
}

fn is_local_address(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => is_local_v4(v4),
        std::net::IpAddr::V6(v6) => is_local_v6(v6),
    }
}

/// IPv4 私有网段（RFC 1918）与链路本地网段（169.254/16）。
///
/// Docker 默认网桥 `172.17.0.0/16` 同样落在 `172.16/12` 里，无法与公司网络的
/// `172.16/12` 区分，因此不做特殊排除——多识别一个“在线”只会多试一条线路。
fn is_local_v4(ip: &std::net::Ipv4Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    ip.is_private() || ip.is_link_local()
}

/// IPv6 唯一本地地址（fc00::/7）与链路本地地址（fe80::/10）。
fn is_local_v6(ip: &std::net::Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    let segments = ip.segments();
    (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn private_ipv4_ranges_count_as_local() {
        for ip in ["10.0.0.5", "172.16.3.4", "172.31.255.254", "192.168.1.10"] {
            assert!(is_local_v4(&ip.parse::<Ipv4Addr>().unwrap()), "{ip}");
        }
    }

    #[test]
    fn link_local_ipv4_counts_as_local() {
        assert!(is_local_v4(&"169.254.3.4".parse::<Ipv4Addr>().unwrap()));
    }

    #[test]
    fn loopback_and_public_addresses_do_not_count() {
        assert!(!is_local_v4(&Ipv4Addr::LOCALHOST));
        assert!(!is_local_v4(&Ipv4Addr::UNSPECIFIED));
        assert!(!is_local_v4(&"8.8.8.8".parse::<Ipv4Addr>().unwrap()));
        assert!(!is_local_v4(&"203.0.113.7".parse::<Ipv4Addr>().unwrap()));
    }

    #[test]
    fn unique_local_and_link_local_ipv6_count_as_local() {
        assert!(is_local_v6(&"fd12:3456::1".parse::<Ipv6Addr>().unwrap()));
        assert!(is_local_v6(&"fe80::1".parse::<Ipv6Addr>().unwrap()));
    }

    #[test]
    fn global_ipv6_and_loopback_do_not_count() {
        assert!(!is_local_v6(&Ipv6Addr::LOCALHOST));
        assert!(!is_local_v6(&"2001:db8::1".parse::<Ipv6Addr>().unwrap()));
    }

    #[test]
    fn the_dispatch_helper_handles_both_address_families() {
        assert!(is_local_address(&"192.168.0.2".parse().unwrap()));
        assert!(is_local_address(&"fd00::2".parse().unwrap()));
        assert!(!is_local_address(&"127.0.0.1".parse().unwrap()));
    }
}
