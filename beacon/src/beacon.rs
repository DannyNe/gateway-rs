use crate::{Entropy, Error, RegionParams, Result};
use byteorder::{ByteOrder, LittleEndian};
use helium_proto::{services::poc_lora, DataRate};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub const BEACON_PAYLOAD_SIZE: usize = 52;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beacon {
    pub data: Vec<u8>,

    pub frequency: u64,
    pub datarate: DataRate,
    pub remote_entropy: Entropy,
    pub local_entropy: Entropy,
    pub conducted_power: u32,
}

impl Beacon {
    /// Construct a new beacon with a given remote and local entropy. The remote
    /// and local entropy are checked for version equality.
    ///
    /// Version 0/1 beacons use a Sha256 of the remote and local entropy (data
    /// and timestamp), which is then used (truncated) as the beacon payload.
    /// The frequency is derived from the first two bytes of the beacon payload,
    /// while the data_rate is derived from the packet size (spreading factor)
    /// and bandwidth as set in the region parameters
    pub fn new(
        remote_entropy: Entropy,
        local_entropy: Entropy,
        region_params: &RegionParams,
    ) -> Result<Self> {
        match remote_entropy.version {
            0 | 1 => {
                let mut data = {
                    let mut hasher = Sha256::new();
                    remote_entropy.digest(&mut hasher);
                    local_entropy.digest(&mut hasher);
                    hasher.finalize().to_vec()
                };

                // Truncate data
                data.truncate(BEACON_PAYLOAD_SIZE);

                // Selet frequency based on the the first two bytes of the
                // beacon data
                let freq_seed = LittleEndian::read_u16(&data) as usize;
                let frequency =
                    region_params.params[freq_seed % region_params.params.len()].channel_frequency;
                let datarate = region_params.select_datarate(data.len())?;
                let conducted_power = region_params.max_conducted_power()?;

                Ok(Self {
                    data,
                    frequency,
                    datarate,
                    local_entropy,
                    remote_entropy,
                    conducted_power,
                })
            }
            _ => Err(Error::invalid_version()),
        }
    }

    pub fn beacon_id(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(&self.data)
    }
}

impl TryFrom<Beacon> for poc_lora::LoraBeaconReportReqV1 {
    type Error = Error;
    fn try_from(v: Beacon) -> Result<Self> {
        Ok(Self {
            pub_key: vec![],
            local_entropy: v.local_entropy.data,
            remote_entropy: v.remote_entropy.data,
            data: v.data,
            frequency: v.frequency,
            channel: 0,
            datarate: v.datarate as i32,
            tmst: 0,
            // This is the initial value. The beacon sender updates this value
            // with the actual conducted power reported by the packet forwarder
            tx_power: v.conducted_power as i32,
            // The timestamp of the beacon is the timestamp of creation of the
            // report (in nanos)
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(Error::from)?
                .as_nanos() as u64,
            signature: vec![],
        })
    }
}
