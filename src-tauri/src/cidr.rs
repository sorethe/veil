use std::fmt;
use std::net::IpAddr;

/// A parsed IPv4 or IPv6 CIDR. Host bits are preserved; overlap checks compare
/// only the bits selected by the shorter prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    pub addr: IpAddr,
    pub prefix: u8,
}

impl Cidr {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (address, prefix) = value
            .split_once('/')
            .ok_or_else(|| format!("'{value}' is missing a /prefix"))?;
        if address.is_empty() {
            return Err(format!("'{value}' is missing an address"));
        }
        if prefix.is_empty() {
            return Err(format!("'{value}' is missing a prefix length"));
        }
        if !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!("'{value}' has a non-numeric prefix length"));
        }
        let addr: IpAddr = address
            .parse()
            .map_err(|_| format!("'{address}' is not an IP address"))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|_| format!("'{value}' has an out-of-range prefix length"))?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(format!(
                "'{value}' uses /{prefix}, which exceeds /{max} for {addr}"
            ));
        }
        Ok(Self { addr, prefix })
    }

    pub fn parse_ipv4(value: &str) -> Result<Self, String> {
        let cidr = Self::parse(value)?;
        if !cidr.addr.is_ipv4() {
            return Err(format!("'{value}' is not an IPv4 CIDR"));
        }
        Ok(cidr)
    }

    pub fn parse_ipv6(value: &str) -> Result<Self, String> {
        let cidr = Self::parse(value)?;
        if !cidr.addr.is_ipv6() {
            return Err(format!("'{value}' is not an IPv6 CIDR"));
        }
        Ok(cidr)
    }

    /// True when the two networks share at least one address. Families never
    /// overlap; within a family the shorter prefix decides the comparison.
    pub fn overlaps(&self, other: &Self) -> bool {
        match (self.addr, other.addr) {
            (IpAddr::V4(left), IpAddr::V4(right)) => overlaps_bits(
                u32::from(left) as u128,
                u32::from(right) as u128,
                self.prefix.min(other.prefix),
                32,
            ),
            (IpAddr::V6(left), IpAddr::V6(right)) => overlaps_bits(
                u128::from(left),
                u128::from(right),
                self.prefix.min(other.prefix),
                128,
            ),
            _ => false,
        }
    }
}

fn overlaps_bits(left: u128, right: u128, prefix: u8, max: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    let shift = max - prefix;
    let mask = (!0u128) << shift;
    (left & mask) == (right & mask)
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cidr(value: &str) -> Cidr {
        Cidr::parse(value).unwrap()
    }

    #[test]
    fn parses_real_cidrs_and_enforces_prefix_bounds() {
        assert_eq!(
            cidr("172.19.0.1/30"),
            Cidr {
                addr: "172.19.0.1".parse().unwrap(),
                prefix: 30
            }
        );
        assert_eq!(cidr("0.0.0.0/0").prefix, 0);
        assert_eq!(cidr("fdfe:dcba:9876::1/126").prefix, 126);
        assert_eq!(cidr("::/0").prefix, 0);

        for invalid in [
            "",
            "172.19.0.1",
            "/30",
            "172.19.0.1/",
            "172.19.0.1/33",
            "172.19.0.1/-1",
            "172.19.0.1/abc",
            "172.19.0.1/30/24",
            "fdfe::1/129",
            "999.1.1.1/24",
        ] {
            assert!(Cidr::parse(invalid).is_err(), "{invalid} must be rejected");
        }
    }

    #[test]
    fn enforces_address_family_for_each_field() {
        assert!(Cidr::parse_ipv4("172.19.0.1/30").is_ok());
        assert!(Cidr::parse_ipv4("fdfe::1/126").is_err());
        assert!(Cidr::parse_ipv6("fdfe::1/126").is_ok());
        assert!(Cidr::parse_ipv6("172.19.0.1/30").is_err());
    }

    #[test]
    fn detects_ipv4_route_overlaps() {
        let proposed = cidr("172.19.0.1/30");
        assert!(proposed.overlaps(&cidr("172.19.0.0/24")));
        assert!(proposed.overlaps(&cidr("172.19.0.2/30")));
        assert!(proposed.overlaps(&cidr("172.16.0.0/12")));
        assert!(proposed.overlaps(&cidr("0.0.0.0/0")));
        assert!(!proposed.overlaps(&cidr("172.19.0.4/30")));
        assert!(!proposed.overlaps(&cidr("10.0.0.0/8")));
        assert!(!proposed.overlaps(&cidr("fdfe::1/126")));
    }

    #[test]
    fn detects_ipv6_route_overlaps() {
        let proposed = cidr("fdfe:dcba:9876::1/126");
        assert!(proposed.overlaps(&cidr("fdfe:dcba:9876::/64")));
        assert!(proposed.overlaps(&cidr("::/0")));
        assert!(!proposed.overlaps(&cidr("fdfe:dcba:9877::/64")));
        assert!(!proposed.overlaps(&cidr("172.19.0.0/30")));
    }

    #[test]
    fn overlap_ignores_host_bits() {
        assert!(cidr("10.0.0.1/24").overlaps(&cidr("10.0.0.200/24")));
        assert!(cidr("10.0.0.200/24").overlaps(&cidr("10.0.0.1/24")));
    }
}
