use std::env;

use serenity::{
    model::{channel::Message, gateway::Ready},
    utils::Colour,
    prelude::*,
    Error as DiscordError,
};
use regex::Regex;
use serenity::model::guild::{Role, Guild};
use serenity::model::id::RoleId;
use err_derive::Error;
use serenity::model::user::User;
use std::u8;
use std::sync::Arc;

#[derive(Debug, Error)]
enum BotError {
    #[error(display = "missing \"colors\" base role")]
    NoColorRole,
    #[error(display = "discord error: {}", _0)]
    DiscordError(#[error(cause)] DiscordError),
}

impl From<DiscordError> for BotError {
    fn from(f: DiscordError) -> Self {
        BotError::DiscordError(f)
    }
}

type Result<T> = std::result::Result<T, BotError>;

struct Handler {
    color_regex: Regex
}

impl Handler {
    pub fn new() -> Self {
        Handler {
            color_regex: Regex::new(r"^#([A-Fa-f0-9]{2})([A-Fa-f0-9]{2})([A-Fa-f0-9]{2})$").unwrap()
        }
    }

    fn parse_color(&self, msg: &str) -> Option<Colour> {
        let captures = self.color_regex.captures(msg)?;
        let r = u8::from_str_radix(captures.get(1).unwrap().as_str(), 16).unwrap();
        let g = u8::from_str_radix(captures.get(2).unwrap().as_str(), 16).unwrap();
        let b = u8::from_str_radix(captures.get(3).unwrap().as_str(), 16).unwrap();
        Some(Colour::from_rgb(r, g, b))
    }

    fn get_color_role_position(&self, guild: &Guild) -> Result<u8> {
        guild.role_by_name("colors").map(|r| r.position as u8)
            .ok_or(BotError::NoColorRole)
    }

    fn get_or_create_role(&self, context: &Context, color: Colour, guild: &RwLock<Guild>) -> Result<Role> {
        let name = format!("#{}", color.hex());
        let color_position = self.get_color_role_position(&mut guild.read())?;
        if let Some(role) = guild.read().role_by_name(&name) {
            return Ok(role.clone());
        }

        let role = guild.write().create_role(context, |r| r
            .name(&name)
            .colour(color.0 as u64)
            .position(color_position),
        )?;

        Ok(role)
    }

    fn assign_color(&self, context: Context, user: User, guild: Arc<RwLock<Guild>>, color: Colour) -> Result<(String, String)> {
        let role = self.get_or_create_role(&context, color, &guild)?;
        let mut member = guild.read().member(&context, user.id)?;

        let old_colors: Vec<RoleId> = member.roles(&context.cache).unwrap_or_default()
            .iter()
            .filter(|r| self.color_regex.is_match(&r.name))
            .map(|r| r.id)
            .collect();
        member.remove_roles(&context.http, &old_colors)?;
        member.add_role(&context.http, role.id)?;
        self.cleanup_roles(context, &guild, role.id)?;
        Ok((role.name, user.name))
    }

    fn cleanup_roles(&self, context: Context, guild: &RwLock<Guild>, used: RoleId) -> Result<()> {
        let used_roles: Vec<RoleId> = guild.read().members.values()
            .flat_map(|member| member.roles.iter())
            .map(|role| role.clone())
            .collect();

        let empty_roles: Vec<RoleId> = guild.read().roles
            .values()
            .filter(|role| self.color_regex.is_match(&role.name))
            .filter(|role| !used_roles.contains(&role.id))
            .filter(|role| role.id != used)
            .map(|role| role.id.clone())
            .collect();

        let guild = guild.write();
        for empty_role in empty_roles {
            guild.delete_role(context.http.clone(), empty_role)?;
        }
        Ok(())
    }
}

impl EventHandler for Handler {
    fn message(&self, context: Context, msg: Message) {
        if let Some(color) = self.parse_color(&msg.content) {
            if let Some(guild) = msg.guild(context.cache.clone()) {
                match self.assign_color(context, msg.author, guild, color) {
                    Ok((role, user)) => {
                        println!("Assigned role {} for {}", role, user);
                    }
                    Err(err) => {
                        println!("Error assigning color: {}", err);
                    }
                }
            } else {
                println!("Failed to get guild");
            }
        }
    }

    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

fn main() {
    env_logger::init().expect("Unable to init env_logger");

    let token = env::var("DISCORD_TOKEN")
        .expect("Expected a token in the environment");

    let mut client = Client::new(&token, Handler::new()).expect("Err creating client");

    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
