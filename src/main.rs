use bytes::{Buf, BufMut, Bytes, BytesMut};

use serenity::{
    async_trait,
    client::{bridge::gateway::GatewayIntents, Client, Context, EventHandler},
    framework::standard::{
        macros::{command, group},
        Args, CommandResult, StandardFramework,
    },
    model::{
        channel::{Channel, Message},
        interactions::application_command::{
            ApplicationCommand, ApplicationCommandInteractionDataOptionValue,
            ApplicationCommandOptionType,
        },
        interactions::{Interaction, InteractionResponseType},
        prelude::Ready,
    },
    prelude::TypeMapKey,
};

use tokio::{net::UdpSocket, time::timeout};

use std::fs;

mod config;

struct ConfigurationContainer;
impl TypeMapKey for ConfigurationContainer {
    type Value = config::Configuration;
}

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            match command.data.name.as_str() {
                "servers" => {
                    let data = ctx.data.read().await;
                    let config = data.get::<ConfigurationContainer>().unwrap();

                    let mut server_info: Vec<(String, String)> = vec![];

                    for server in &config.servers {
                        match query_server(&server.name, format!("{}:{}", &server.ip, &server.port))
                            .await
                        {
                            Ok(res) => {
                                // Server responded properly
                                server_info.push(res);
                            }
                            Err(_) => {
                                // Failed to query server
                                server_info.push((server.name.clone(), "*Offline*".into()));
                            }
                        }
                    }

                    command
                        .create_interaction_response(&ctx.http, |response| {
                            response
                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|m| {
                                    m.create_embed(|e| {
                                        e.title("Breadpudding's Server Status");

                                        for item in &server_info {
                                            e.field(&item.0, &item.1, false);
                                        }
                                        e
                                    })
                                })
                        })
                        .await
                        .expect("Failed to send interaction response");
                }
                "role" => {
                    let options = command
                        .data
                        .options
                        .get(0)
                        .expect("Interaction received was invalid")
                        .resolved
                        .as_ref()
                        .expect("Role object missing");

                    if let ApplicationCommandInteractionDataOptionValue::Role(role) = options {
                        let data = ctx.data.read().await;
                        let config = data.get::<ConfigurationContainer>().unwrap();

                        if config.guild_id.parse::<u64>().unwrap() == role.guild_id.0 {
                            // Check if the role can be added

                            // Get user as guild member
                            let guild = ctx
                                .http
                                .get_guild(config.guild_id.parse::<u64>().unwrap())
                                .await
                                .expect("Could not find guild");
                            let mut member = guild
                                .member(&ctx.http, command.member.clone().unwrap().user.id)
                                .await
                                .unwrap();

                            let mut desired_role = None;

                            'outer: for category in &config.role {
                                for allowed_role in &category.names {
                                    if role.name.to_lowercase() == allowed_role.to_lowercase() {
                                        desired_role = Some(role.clone());
                                        break 'outer;
                                    }
                                }
                            }

                            let text_response = if desired_role.is_some() {
                                if member.roles.contains(&role.id) {
                                    member.remove_role(&ctx, role).await.unwrap();
                                    format!("Removed role `{}`", role.name)
                                } else {
                                    member.add_role(&ctx, role).await.unwrap();
                                    format!("Added role `{}`", role.name)
                                }
                            } else {
                                "The role you have requested cannot be found".into()
                            };

                            command
                                .create_interaction_response(&ctx, |response| {
                                    response
                                        .kind(InteractionResponseType::ChannelMessageWithSource)
                                        .interaction_response_data(|m| m.content(text_response))
                                })
                                .await
                                .expect("Failed to send interaction response");
                        }
                    }
                }
                "listroles" => {
                    let data = ctx.data.read().await;
                    let config = data.get::<ConfigurationContainer>().unwrap();

                    command
                        .create_interaction_response(&ctx.http, |response| {
                            response
                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|m| {
                                    // Create the embed
                                    m.create_embed(|e| {
                                        e.color(0x374c0c);
                                        e.title("Available Roles");
                                        e.description(format!(
                                            "Modify your roles with:\n{}role add/remove <role>",
                                            &config.prefix
                                        ));

                                        // Loop through all of the roles and develop a field for each category
                                        for category in &config.role {
                                            let mut role_list = "".to_string();
                                            for role in &category.names {
                                                role_list += &format!("{}\n", role);
                                            }
                                            e.field(&category.category, role_list, false);
                                        }

                                        e
                                    })
                                })
                        })
                        .await
                        .expect("Failed to send interaction response");
                }
                _ => {
                    println!("Unknown interaction command received")
                }
            };
        }
    }

    async fn ready(&self, ctx: Context, _ready: Ready) {
        let commands = ApplicationCommand::set_global_application_commands(&ctx.http, |cmds| {
            cmds.create_application_command(|cmd| {
                cmd.name("role")
                    .description("Add or remove a role")
                    .create_option(|opt| {
                        opt.name("role")
                            .description("The role to add or remove")
                            .kind(ApplicationCommandOptionType::Role)
                            .required(true)
                    })
            })
            .create_application_command(|cmd| {
                cmd.name("listroles").description("List available roles")
            })
            .create_application_command(|cmd| {
                cmd.name("servers")
                    .description("List the status of the servers")
            })
        })
        .await
        .unwrap();

        println!("Registered commands: {:#?}", commands);
    }
}

#[group]
#[commands(role, servers)]
struct Commands;

#[command]
async fn role(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    // Only fetch data before needed
    let data = ctx.data.read().await;
    let config = data.get::<ConfigurationContainer>().unwrap();

    if args.is_empty() {
        // List roles
        if let Channel::Guild(channel) = msg.channel(ctx).await.unwrap() {
            channel
                .send_message(ctx, |m| {
                    // Create the embed
                    m.embed(|e| {
                        e.color(0x374c0c);
                        e.title("Available Roles");
                        e.description(format!(
                            "Modify your roles with:\n{}role add/remove <role>",
                            &config.prefix
                        ));

                        // Loop through all of the roles and develop a field for each category
                        for category in &config.role {
                            let mut role_list = "".to_string();
                            for role in &category.names {
                                role_list += &format!("{}\n", role);
                            }
                            e.field(&category.category, role_list, false);
                        }

                        e
                    })
                })
                .await
                .unwrap();
        }
        Ok(())
    } else {
        let role_name = args.rest();

        let mut desired_role = None;

        'outer: for category in &config.role {
            for role in &category.names {
                if role.to_lowercase() == role_name.to_lowercase() {
                    desired_role = Some(role.clone());
                    break 'outer;
                }
            }
        }

        if desired_role.is_some() {
            let guild = msg.guild(ctx).await.unwrap();
            let role = guild.role_by_name(&desired_role.unwrap());
            if role.is_some() {
                let mut member = guild.member(ctx, msg.author.id).await.unwrap();
                let roles = member.roles(ctx).await.unwrap();

                let role = role.unwrap();
                if roles.contains(role) {
                    // Remove role
                    member.remove_role(ctx, role).await.unwrap();
                    msg.reply(ctx, format!("Removed role `{}`", role.name))
                        .await
                        .unwrap();
                } else {
                    // Add role
                    member.add_role(ctx, role).await.unwrap();
                    msg.reply(ctx, format!("Added role `{}`", role.name))
                        .await
                        .unwrap();
                }
            } else {
                // Role by name returned nothing D:
                msg.reply(ctx, "An issue occured attempting to find the role")
                    .await
                    .unwrap();
            }
        } else {
            // The role requested did not match any available (case insensitive)
            msg.reply(ctx, "The role you have requested cannot be found")
                .await
                .unwrap();
        }

        Ok(())
    }
}

// Helper function to read null-terminated strings from the server query
fn read_string(bytes: &mut Bytes) -> String {
    let mut string = Vec::new();
    let mut byte;
    loop {
        byte = bytes.get_u8();
        if byte == 0x00 {
            break;
        } else {
            string.push(byte);
        }
    }
    String::from_utf8_lossy(&string).into()
}

async fn query_server(
    name: &str,
    address: String,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    // Bind to any port and address and connect to the server
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(&address).await?;

    // Create query packet
    let mut buf = BytesMut::with_capacity(29);
    buf.put_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // Header
    buf.put_u8(0x54); // A2S_INFO
    buf.put_slice(b"Source Engine Query\0"); // Exists for some reason
    buf.put_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // Default challenge

    // Send the A2S_INFO packet
    socket.send(&buf).await?;

    let mut reply_buf = [0u8; 1024];

    let recv_future = socket.recv(&mut reply_buf);
    // Throw an error if a response is not recieved within 300ms
    timeout(std::time::Duration::from_millis(300), recv_future).await??;

    // Replace reply_buf with a helpful Bytes buffer
    let mut reply_buf = Bytes::copy_from_slice(&reply_buf);

    if reply_buf.get_u32() == 0xFFFFFFFF {
        let packet_type = reply_buf.get_u8(); // A2S_INFO should be 0x49
        if packet_type != 0x49 {
            // Throw error
            return Ok((name.into(), "*Invalid response received from server".into()));
        }
        let _protocol = reply_buf.get_u8();
        let name = read_string(&mut reply_buf);
        let map = read_string(&mut reply_buf);
        let _folder = read_string(&mut reply_buf);
        let _game = read_string(&mut reply_buf);
        let _steamid = reply_buf.get_u16();
        let players = reply_buf.get_u8();
        let max_players = reply_buf.get_u8();
        // There is more, but I do not see more important data

        Ok((
            name,
            format!(
                "| {} | {}/{} | Join: steam://connect/{} |",
                map, players, max_players, address
            ),
        ))
    } else {
        Ok((name.into(), "*Invalid response received from server".into()))
    }
}

#[command]
async fn servers(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read().await;
    let config = data.get::<ConfigurationContainer>().unwrap();

    let mut server_info: Vec<(String, String)> = vec![];

    for server in &config.servers {
        match query_server(&server.name, format!("{}:{}", &server.ip, &server.port)).await {
            Ok(res) => {
                // Server responded properly
                server_info.push(res);
            }
            Err(_) => {
                // Failed to query server
                server_info.push((server.name.clone(), "*Offline*".into()));
            }
        }
    }

    if let Channel::Guild(channel) = msg.channel(ctx).await.unwrap() {
        channel
            .send_message(ctx, |m| {
                m.embed(|e| {
                    e.color(0x374c0c);
                    e.title("Breadpudding's Server Status");

                    for item in &server_info {
                        e.field(&item.0, &item.1, false);
                    }
                    e
                })
            })
            .await
            .unwrap();
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    // Read in configuration from config.toml
    let config_raw = fs::read_to_string("config.toml").expect("Unable to read configuration file");
    let config: config::Configuration =
        toml::from_str(&config_raw).expect("Unable to parse configuration file");

    let mut client = Client::builder(&config.token)
        .application_id(config.application_id.parse::<u64>().unwrap())
        .event_handler(Handler)
        .framework(
            StandardFramework::new()
                .configure(|c| c.with_whitespace(true).prefix(&config.prefix))
                .group(&COMMANDS_GROUP),
        )
        .intents(
            GatewayIntents::GUILD_MESSAGES | GatewayIntents::GUILD_MEMBERS | GatewayIntents::GUILDS,
        )
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ConfigurationContainer>(config);
    }

    if let Err(why) = client.start().await {
        println!("Error: {:?}", why);
    }
}
