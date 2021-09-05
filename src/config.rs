use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct RoleConfiguration {
    pub category: String,
    pub names: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ServerConfiguration {
    pub name: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub token: String,
    pub prefix: String,
    pub guild_id: String,
    pub application_id: String,

    pub servers: Vec<ServerConfiguration>,

    // Vec<(`Category`, Vec<`role ids`>)>
    pub role: Vec<RoleConfiguration>,
}
