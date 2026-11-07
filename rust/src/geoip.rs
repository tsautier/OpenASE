//! IP to country lookup with the DB-IP "IP to Country Lite" database.
//!
//! The database is published monthly by DB-IP.com under the Creative Commons
//! Attribution 4.0 license (<https://db-ip.com>). OpenASE downloads it into
//! its settings folder on first use and refreshes it once a month; nothing is
//! bundled in the repository.

use std::io::{self, Read};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

/// Attribution required by the CC BY 4.0 license.
pub const ATTRIBUTION: &str = "IP Geolocation by DB-IP";
pub const ATTRIBUTION_URL: &str = "https://db-ip.com";

const MAX_AGE: Duration = Duration::from_secs(32 * 24 * 3600);
const CACHE_FILE: &str = "dbip-country-lite.csv.gz";

/// Sorted IPv4 ranges mapped to ISO country codes.
#[derive(Debug, Default, Clone)]
pub struct GeoIp {
    starts: Vec<u32>,
    ends: Vec<u32>,
    codes: Vec<[u8; 2]>,
}

impl GeoIp {
    /// Parse DB-IP CSV text (`first,last,country`). IPv6 lines are skipped.
    pub fn parse(text: &str) -> Self {
        let mut ranges: Vec<(u32, u32, [u8; 2])> = text
            .lines()
            .filter_map(|line| {
                let mut fields = line.split(',');
                let start: Ipv4Addr = fields.next()?.trim().parse().ok()?;
                let end: Ipv4Addr = fields.next()?.trim().parse().ok()?;
                let code = fields.next()?.trim().trim_matches('"').as_bytes();
                (code.len() == 2).then(|| (u32::from(start), u32::from(end), [code[0], code[1]]))
            })
            .collect();
        ranges.sort_unstable_by_key(|range| range.0);
        Self {
            starts: ranges.iter().map(|range| range.0).collect(),
            ends: ranges.iter().map(|range| range.1).collect(),
            codes: ranges.iter().map(|range| range.2).collect(),
        }
    }

    /// Load a gzip-compressed DB-IP CSV file.
    pub fn from_gz_file(path: &Path) -> io::Result<Self> {
        let mut text = String::new();
        GzDecoder::new(std::fs::File::open(path)?).read_to_string(&mut text)?;
        let geoip = Self::parse(&text);
        if geoip.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "GeoIP file has no IPv4 ranges",
            ));
        }
        Ok(geoip)
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// ISO country code for an address, or `None` when unknown or reserved.
    pub fn lookup(&self, ip: Ipv4Addr) -> Option<&str> {
        let ip = u32::from(ip);
        let index = self
            .starts
            .partition_point(|&start| start <= ip)
            .checked_sub(1)?;
        if ip > self.ends[index] {
            return None;
        }
        let code = std::str::from_utf8(&self.codes[index]).ok()?;
        (code != "ZZ").then_some(code)
    }

    /// Look up a host given as text; hostnames are not resolved.
    pub fn lookup_host(&self, host: &str) -> Option<&str> {
        host.parse().ok().and_then(|ip| self.lookup(ip))
    }
}

/// Where the downloaded database is cached.
pub fn cache_path() -> PathBuf {
    crate::settings::config_dir().join(CACHE_FILE)
}

/// Download URL for a given month.
pub fn download_url(year: i32, month: u32) -> String {
    format!("https://download.db-ip.com/free/dbip-country-lite-{year}-{month:02}.csv.gz")
}

/// Current UTC year and month.
pub fn current_year_month() -> (i32, u32) {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    year_month_from_days(days)
}

/// Civil year and month for a count of days since 1970-01-01
/// (Howard Hinnant's `civil_from_days`).
pub fn year_month_from_days(days: i64) -> (i32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month)
}

fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

fn is_fresh(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < MAX_AGE)
}

fn download_to(url: &str, path: &Path, timeout: Duration) -> io::Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(io::Error::other)?;
    let partial = path.with_extension("part");
    {
        let mut file = std::fs::File::create(&partial)?;
        io::copy(&mut response.body_mut().as_reader(), &mut file)?;
    }
    // Only replace the cache with a file that parses.
    GeoIp::from_gz_file(&partial)?;
    std::fs::rename(&partial, path)
}

/// Load the cached database, downloading this month's (or last month's)
/// edition when the cache is missing or older than a month.
pub fn load_or_download(timeout: Duration) -> io::Result<GeoIp> {
    let path = cache_path();
    if !is_fresh(&path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let (year, month) = current_year_month();
        let (prev_year, prev_month) = previous_month(year, month);
        let downloaded = download_to(&download_url(year, month), &path, timeout)
            .or_else(|_| download_to(&download_url(prev_year, prev_month), &path, timeout));
        if let Err(error) = downloaded
            && !path.is_file()
        {
            return Err(error);
        }
    }
    GeoIp::from_gz_file(&path)
}
