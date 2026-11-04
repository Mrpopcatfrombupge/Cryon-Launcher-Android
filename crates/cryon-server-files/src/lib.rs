//! The dedicated server's files and settings, ported from
//! net.minecraft.server.dedicated and net.minecraft.server.players:
//! server.properties, eula.txt, the four user lists, usercache.json and
//! looking players up by name. The game's dedicated server
//! (`cryon-client --dedicated`) reads them, and `cryon-server` manages
//! them; this crate needs neither a window nor the game.
pub mod eula;
pub mod java_properties;
pub mod json;
pub mod profiles;
pub mod properties;
pub mod user_cache;
pub mod user_lists;
