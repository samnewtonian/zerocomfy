/// mDNS service type for authority self-advertisement
pub const AUTHORITY_SERVICE_TYPE: &str = "_subnet-authority._tcp.local.";

/// TXT record keys used in authority self-advertisement
pub const TXT_ZONE: &str = "zone";
pub const TXT_PREFIX: &str = "prefix";
pub const TXT_COAP_PORT: &str = "coap";
pub const TXT_DNS_PORT: &str = "dns";

/// API path prefix
pub const API_PREFIX: &str = "/v1";
