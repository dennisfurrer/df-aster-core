//! Where Aster is: its REST hosts, its websocket, and the chain ids its signatures name. Every
//! host is a field, so a program may put a proxy in front of one (a browser reads `fundingRate`,
//! which Aster serves without CORS headers, through one).

/// Aster's futures API.
pub const REST_URL: &str = "https://fapi.asterdex.com";

/// Aster Chain's API: withdrawals and transfers on the chain.
pub const CHAIN_URL: &str = "https://chainapi.asterdex.com";

/// The web app's API, under `/bapi`: deposit and withdrawal asset lists.
pub const WEB_URL: &str = "https://www.asterdex.com";

/// The futures websocket: market streams at `/stream` (combined) or `/ws/<stream>`, an
/// account's stream at `/ws/<listenKey>`.
pub const WS_URL: &str = "wss://fstream.asterdex.com";

/// The chain the main wallet signs agent approvals and withdrawals on: BNB Smart Chain.
pub const WALLET_CHAIN_ID: u64 = 56;

/// What `asterChain` names in what the main wallet signs.
pub const ASTER_CHAIN: &str = "Mainnet";

/// The roots every call and socket goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hosts {
    /// The futures API, `/fapi/v3/*`.
    pub rest: String,
    /// Aster Chain's API, `/aster-chain/v3/*`.
    pub chain: String,
    /// The web app's API, `/bapi/*`.
    pub web: String,
    /// The futures websocket.
    pub ws: String,
}

impl Hosts {
    /// Aster's own hosts.
    #[must_use]
    pub fn mainnet() -> Self {
        Hosts { rest: REST_URL.to_owned(), chain: CHAIN_URL.to_owned(), web: WEB_URL.to_owned(), ws: WS_URL.to_owned() }
    }

    /// The same hosts with the futures API at `rest`: a proxy, or a test's fake venue.
    #[must_use]
    pub fn with_rest(mut self, rest: impl Into<String>) -> Self {
        self.rest = rest.into().trim_end_matches('/').to_owned();
        self
    }

    /// The same hosts with every REST call at `root`: a test's fake venue.
    #[must_use]
    pub fn all_at(root: impl Into<String>) -> Self {
        let root = root.into().trim_end_matches('/').to_owned();
        Hosts { rest: root.clone(), chain: root.clone(), web: root, ws: WS_URL.to_owned() }
    }

    /// The combined market stream: frames arrive as `{"stream": ..., "data": ...}`.
    #[must_use]
    pub fn market_stream(&self) -> String {
        format!("{}/stream", self.ws)
    }

    /// An account's stream, for a listen key from `POST /fapi/v3/listenKey`.
    #[must_use]
    pub fn user_stream(&self, listen_key: &str) -> String {
        format!("{}/ws/{listen_key}", self.ws)
    }
}

impl Default for Hosts {
    fn default() -> Self {
        Hosts::mainnet()
    }
}
