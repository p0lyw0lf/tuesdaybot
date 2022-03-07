use std::{fs::File, io::prelude::*};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Weekday};
use regex::RegexSet;
use serenity::{
    model::{channel::Message, gateway::Ready, id::RoleId},
    prelude::*,
    utils::MessageBuilder,
};

const TUESDAY_GROUP_ID: u64 = 709526709187248241;

const TIME_UNITS: [&str; 6] = [r"sec", r"min", r"hour", r" day|days", r"week", r"year"];
const TIME_UNITS_PLURAL: [&str; 6] = ["seconds", "minutes", "hours", "days", "weeks", "years"];
const TIME_MULTIPLIERS: [i64; 6] = [1, 60, 3600, 86400, 604800, 31557600];

const DEFAULT_TIME_INDEX: usize = 2;

const SI_UNITS: [&str; 20] = [
    r"yocto", r"zepto", r"atto", r"femto", r"pico", r"nano", r"micro", r"milli", r"centi", r"deci",
    r"deca", r"hecto", r"kilo", r"mega", r"giga", r"tera", r"peta", r"exa", r"zetta", r"yotta",
];

const SI_POWERS: [i32; 20] = [
    -24, -21, -18, -15, -12, -9, -6, -3, -2, -1, 1, 2, 3, 6, 9, 12, 15, 18, 21, 24,
];

struct CustomRegexCache {
    time_regex: RegexSet,
    si_regex: RegexSet,
}

struct RegexKey;
impl TypeMapKey for RegexKey {
    type Value = CustomRegexCache;
}

struct Handler;

impl Handler {
    fn initialize_regex(client: &Client) {
        let mut data = client.data.write();
        let time_regex = RegexSet::new(&TIME_UNITS).expect("Error building time regexes");
        let si_regex = RegexSet::new(&SI_UNITS).expect("Error building si regexes");

        data.insert::<RegexKey>(CustomRegexCache {
            time_regex: time_regex,
            si_regex: si_regex,
        });
    }

    fn handle_tuesday(s: String, ctx: &Context, msg: Message) {
        // First, get the local time
        let now = Local::now().naive_local();

        // Then, calculate when the next tuesday will be
        let num_days_increment = match now.weekday() {
            Weekday::Mon => 1,
            Weekday::Tue => 0,
            Weekday::Wed => 6,
            Weekday::Thu => 5,
            Weekday::Fri => 4,
            Weekday::Sat => 3,
            Weekday::Sun => 2,
        };

        let tuesday: NaiveDateTime = match now.with_ordinal(now.ordinal() + num_days_increment) {
            // Account for the year boundary, get the first tuesday
            // next year in that case
            None => NaiveDate::from_weekday_of_month(now.year() + 1, 1, Weekday::Tue, 1),
            Some(t) => NaiveDate::from_yo(t.year(), t.ordinal()),
        }
        .and_hms_micro(0, 0, 0, 0);
        // Finally get how long it will be until the start of that tuesday
        let diff = tuesday.signed_duration_since(now);
        let diffms = diff.num_milliseconds() as f64;

        // Start constructing the output message
        let tuesday_role_id : RoleId = TUESDAY_GROUP_ID.into();
        let mut output: String = "".to_string();
        
        // Check that we can mention and are in the same guild
        if let Some(role) = tuesday_role_id.to_role_cached(ctx.cache.as_ref()) {
            if role.mentionable {
                match role.find_guild(ctx.cache.as_ref()) {
                    Ok(guild_id) => {
                        if let Some(msg_guild_id) = msg.guild_id {
                            if guild_id == msg_guild_id {
                                output.push_str(format!("{} ", tuesday_role_id.mention()).as_str());
                            } else {
                                println!("Tuesdaybot activated in guild {}, but wants to be in {}", msg_guild_id, guild_id);
                            }
                        }
                    },
                    Err(why) => {
                        println!("Error getting guild_id of TUESDAY_GROUP_ID role: {:?}", why);
                    }
                };
            }
        }

        output.push_str("It is ");

        let (multiplier, unit_string) = Handler::find_multiplier_from(s, &ctx);

        let adjusted_diff = diffms / multiplier;
        output.push_str(format!("{} {} until Tuesday.", adjusted_diff, unit_string).as_str());

        // Sending a message can fail, due to a network error, an
        // authentication error, or lack of permissions to post in the
        // channel, so log to stdout when some error happens, with a
        // description of it.
        if let Err(why) = msg.channel_id.say(&ctx.http, output) {
            println!("Error sending message: {:?}", why);
        }
    }

    fn find_multiplier_from(s: String, ctx: &Context) -> (f64, String) {
        // Load regexes from cache, find all matches in the string.
        let data = ctx.data.read();
        let regex_cache: &CustomRegexCache = data
            .get::<RegexKey>()
            .expect("Expected to find cached regexes in context");
        let time_matches = regex_cache.time_regex.matches(&s);
        let si_matches = regex_cache.si_regex.matches(&s);

        // Prioritize the longest time amounts, defaulting to hours if nothing found
        let time_index: usize = match time_matches.iter().next_back() {
            None => DEFAULT_TIME_INDEX,
            Some(i) => i,
        };

        // For all powers mentioned, add it to the multiplier and also to a prefix string
        let mut si_power = 3;
        let mut unit_string = String::new();
        for i in si_matches.iter() {
            si_power += SI_POWERS[i];
            unit_string.push_str(SI_UNITS[i]);
        }
        unit_string.push_str(TIME_UNITS_PLURAL[time_index]);

        let multiplier = (TIME_MULTIPLIERS[time_index] as f64) * (10 as f64).powf(si_power as f64);

        return (multiplier, unit_string);
    }
}

impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        let mut s = String::from(&msg.content);
        s.make_ascii_lowercase();

        if s.starts_with("tue!") {
            let rest = s.split_off(4);
            if rest.starts_with("role") {
                let mut builder = MessageBuilder::new();
                builder.push("Roles mentioned:\n");
                for roleid in &msg.mention_roles {
                    builder.push(&format!("{}\n", roleid).to_string());
                }
                if let Err(why) = msg.channel_id.say(&ctx.http, &builder.build()) {
                    println!("Error sending message: {:?}", why);
                }
            }
        } else {
            // These repeated string searches could be optimized
            if s.contains("tues") {
                Handler::handle_tuesday(s, &ctx, msg);
            }
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

fn main() {
    println!("Attempting to load token");
    // Configure bot with token read from file
    let mut file = File::open("oauth2.tok").expect("Error opening oauth2.tok");
    let mut token = String::new();
    file.read_to_string(&mut token)
        .expect("Error reading oauth2.tok");

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    println!("Attempting to create client struct");
    let mut client = Client::new(&token, Handler).expect("Err creating client");

    // Compile and add regexes to the cache
    println!("Initializing Regexes");
    Handler::initialize_regex(&client);

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    println!("Attempting to start client");
    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
