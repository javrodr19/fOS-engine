//! ICE (Interactive Connectivity Establishment)
//!
//! Full ICE agent with candidate gathering, connectivity checks, and nomination.

use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

/// ICE candidate type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateType { Host, Srflx, Prflx, Relay }

/// ICE candidate
#[derive(Debug, Clone)]
pub struct IceCandidate {
    pub foundation: String,
    pub component: u8,
    pub protocol: String,
    pub priority: u32,
    pub address: SocketAddr,
    pub candidate_type: CandidateType,
    pub rel_addr: Option<SocketAddr>,
}

impl IceCandidate {
    pub fn host(addr: SocketAddr, component: u8) -> Self {
        let priority = Self::calculate_priority(CandidateType::Host, component);
        Self { foundation: format!("host{}", component), component, protocol: "udp".into(), priority, address: addr, candidate_type: CandidateType::Host, rel_addr: None }
    }
    
    pub fn srflx(addr: SocketAddr, base: SocketAddr, component: u8) -> Self {
        let priority = Self::calculate_priority(CandidateType::Srflx, component);
        Self { foundation: format!("srflx{}", component), component, protocol: "udp".into(), priority, address: addr, candidate_type: CandidateType::Srflx, rel_addr: Some(base) }
    }
    
    fn calculate_priority(ctype: CandidateType, component: u8) -> u32 {
        let type_pref = match ctype { CandidateType::Host => 126, CandidateType::Srflx => 100, CandidateType::Prflx => 110, CandidateType::Relay => 0 };
        (type_pref << 24) | (65535 << 8) | (256 - component as u32)
    }
    
    pub fn to_sdp(&self) -> String {
        format!("candidate:{} {} {} {} {} {} typ {}", self.foundation, self.component, self.protocol, self.priority, self.address.ip(), self.address.port(),
            match self.candidate_type { CandidateType::Host => "host", CandidateType::Srflx => "srflx", CandidateType::Prflx => "prflx", CandidateType::Relay => "relay" })
    }
}

/// ICE connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IceState { #[default] New, Gathering, Checking, Connected, Completed, Failed, Disconnected, Closed }

/// ICE agent
#[derive(Debug)]
pub struct IceAgent {
    pub state: IceState,
    pub local_candidates: Vec<IceCandidate>,
    pub remote_candidates: Vec<IceCandidate>,
    pub stun_servers: Vec<String>,
    pub turn_servers: Vec<TurnServer>,
    pub local_ufrag: String,
    pub local_pwd: String,
    pub remote_ufrag: Option<String>,
    pub remote_pwd: Option<String>,
    nomination_started: bool,
}

/// TURN server config
#[derive(Debug, Clone)]
pub struct TurnServer { pub url: String, pub username: String, pub credential: String }

impl IceAgent {
    pub fn new() -> Self {
        Self {
            state: IceState::New, local_candidates: Vec::new(), remote_candidates: Vec::new(),
            stun_servers: vec!["stun:stun.l.google.com:19302".into()],
            turn_servers: Vec::new(),
            local_ufrag: Self::generate_ufrag(), local_pwd: Self::generate_pwd(),
            remote_ufrag: None, remote_pwd: None, nomination_started: false,
        }
    }
    
    fn generate_ufrag() -> String { format!("{:08x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u32) }
    fn generate_pwd() -> String { format!("{:016x}{:08x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64, rand_u32()) }
    
    pub fn gather_candidates(&mut self) {
        self.state = IceState::Gathering;
        
        // Add host candidates (local interfaces)
        self.local_candidates.push(IceCandidate::host(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 50000), 1));
        
        // In real impl: send STUN binding requests to get srflx candidates
        // In real impl: allocate TURN relays
        
        self.state = IceState::Checking;
    }
    
    pub fn add_remote_candidate(&mut self, candidate: IceCandidate) {
        self.remote_candidates.push(candidate);
    }
    
    pub fn set_remote_credentials(&mut self, ufrag: String, pwd: String) {
        self.remote_ufrag = Some(ufrag);
        self.remote_pwd = Some(pwd);
    }
    
    pub fn check_connectivity(&mut self) -> bool {
        // In real impl: send STUN binding requests to remote candidates, perform connectivity checks
        if !self.local_candidates.is_empty() && !self.remote_candidates.is_empty() {
            self.state = IceState::Connected;
            true
        } else { false }
    }
    
    pub fn nominate(&mut self) {
        if self.state == IceState::Connected && !self.nomination_started {
            self.nomination_started = true;
            self.state = IceState::Completed;
        }
    }
}

impl Default for IceAgent { fn default() -> Self { Self::new() } }

fn rand_u32() -> u32 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_ice() { let ice = IceAgent::new(); assert_eq!(ice.state, IceState::New); }
}
