//! Names for the four-character codes the original gives its data and its objects.
//!
//! Abilities, buffs, unit and item types, and upgrade effects are named as the game's
//! data names them (each by its editor name, a creep's or an item's variant marked so);
//! the engine's own object classes as the saves and traces show them (a few names
//! provisional, as noted).  Each data code is here twice: as text, as the data's fields
//! hold it (`ability::DEVOTION_AURA`, "AHad"), and as an id (`ability::id::DEVOTION_AURA`,
//! b"AHad").  Search for a code (e.g., `AHad`) to find its name.

/// A four-character code as the engine keeps it.
pub type Code = [u8; 4];

/// A code's id from its text.
pub const fn id(text: &str) -> Code {
    let b = text.as_bytes();
    assert!(b.len() == 4);
    [b[0], b[1], b[2], b[3]]
}

/// Each code as text, and in `id` the same as ids.
macro_rules! codes {
    ($($(#[$doc:meta])* $name:ident = $text:literal;)*) => {
        $($(#[$doc])* pub const $name: &str = $text;)*
        /// The same codes as ids.
        pub mod id {
            $($(#[$doc])* pub const $name: &$crate::codes::Code = &$crate::codes::id($text);)*
        }
    };
}

/// Abilities (AbilityData), aliases and the codes of the kinds they are.
pub mod ability {
    codes! {
        /// `ABnP`: Build In Progress (a structure's while it is built; the engine's).
        BUILD_IN_PROGRESS = "ABnP";
        /// `ACad`: Animate Dead (creep).
        ANIMATE_DEAD_CREEP = "ACad";
        /// `ACbf`: Breath of Frost(Creep).
        BREATH_OF_FROST_CREEP = "ACbf";
        /// `ACdv`: Devour (Dragon Creep).
        DEVOUR_DRAGON_CREEP = "ACdv";
        /// `ACmi`: Magic Immunity (Creep).
        MAGIC_IMMUNITY_CREEP = "ACmi";
        /// `ACrn`: Reincarnation (creep).
        REINCARNATION_CREEP = "ACrn";
        /// `ACsp`: Creep Sleep.
        CREEP_SLEEP = "ACsp";
        /// `ACtb`: Thunder Bolt (Creep).
        THUNDER_BOLT_CREEP = "ACtb";
        /// `ACtc`: Thunder Clap (Creep).
        THUNDER_CLAP_CREEP = "ACtc";
        /// `AEah`: Keeper - Thorns Aura.
        KEEPER_THORNS_AURA = "AEah";
        /// `AEar`: Priestess - Trueshot Aura.
        PRIESTESS_TRUESHOT_AURA = "AEar";
        /// `AEbl`: Warden - Blink.
        WARDEN_BLINK = "AEbl";
        /// `AEbu`: Build (Night Elf).
        BUILD_NIGHT_ELF = "AEbu";
        /// `AEer`: Keeper - Entangling Roots.
        KEEPER_ENTANGLING_ROOTS = "AEer";
        /// `AEev`: Demon Hunter - Evasion.
        DEMON_HUNTER_EVASION = "AEev";
        /// `AEfk`: Warden - Fan of Knives.
        WARDEN_FAN_OF_KNIVES = "AEfk";
        /// `AEfn`: Keeper - Force of Nature.
        KEEPER_FORCE_OF_NATURE = "AEfn";
        /// `AEim`: Demon Hunter - Immolation.
        DEMON_HUNTER_IMMOLATION = "AEim";
        /// `AEmb`: Demon Hunter - Mana Burn.
        DEMON_HUNTER_MANA_BURN = "AEmb";
        /// `AEme`: Demon Hunter - Metamorphosis.
        DEMON_HUNTER_METAMORPHOSIS = "AEme";
        /// `AEsf`: Priestess - Starfall.
        PRIESTESS_STARFALL = "AEsf";
        /// `AEsh`: Warden - Shadow Strike.
        WARDEN_SHADOW_STRIKE = "AEsh";
        /// `AEst`: Priestess - Scout.
        PRIESTESS_SCOUT = "AEst";
        /// `AEsv`: Warden - Spirit of Vengeance.
        WARDEN_SPIRIT_OF_VENGEANCE = "AEsv";
        /// `AEtq`: Keeper - Tranquility.
        KEEPER_TRANQUILITY = "AEtq";
        /// `AGbu`: Build (Naga).
        BUILD_NAGA = "AGbu";
        /// `AHab`: Arch Mage - Brilliance Aura.
        ARCH_MAGE_BRILLIANCE_AURA = "AHab";
        /// `AHad`: Paladin - Devotion Aura.
        PALADIN_DEVOTION_AURA = "AHad";
        /// `AHav`: Mountain King - Avatar.
        MOUNTAIN_KING_AVATAR = "AHav";
        /// `AHbh`: Mountain King - Bash.
        MOUNTAIN_KING_BASH = "AHbh";
        /// `AHbn`: Blood Mage - Banish.
        BLOOD_MAGE_BANISH = "AHbn";
        /// `AHbu`: Build (Human).
        BUILD_HUMAN = "AHbu";
        /// `AHbz`: Arch Mage - Blizzard.
        ARCH_MAGE_BLIZZARD = "AHbz";
        /// `AHca`: Cold Arrows.
        COLD_ARROWS = "AHca";
        /// `AHdr`: Blood Mage - Siphon Mana.
        BLOOD_MAGE_SIPHON_MANA = "AHdr";
        /// `AHds`: Paladin - Divine Shield.
        PALADIN_DIVINE_SHIELD = "AHds";
        /// `AHer`: Hero.
        HERO = "AHer";
        /// `AHfa`: Priestess - Searing Arrows.
        PRIESTESS_SEARING_ARROWS = "AHfa";
        /// `AHfs`: Blood Mage - Flame Strike.
        BLOOD_MAGE_FLAME_STRIKE = "AHfs";
        /// `AHhb`: Paladin - Holy Light.
        PALADIN_HOLY_LIGHT = "AHhb";
        /// `AHmt`: Arch Mage - Mass Teleport.
        ARCH_MAGE_MASS_TELEPORT = "AHmt";
        /// `AHpx`: Blood Mage - Phoenix.
        BLOOD_MAGE_PHOENIX = "AHpx";
        /// `AHre`: Paladin - Resurrection.
        PALADIN_RESURRECTION = "AHre";
        /// `AHtb`: Mountain King - Thunder Bolt.
        MOUNTAIN_KING_THUNDER_BOLT = "AHtb";
        /// `AHtc`: Mountain King - Thunder Clap.
        MOUNTAIN_KING_THUNDER_CLAP = "AHtc";
        /// `AHwe`: Arch Mage - Water Elemental.
        ARCH_MAGE_WATER_ELEMENTAL = "AHwe";
        /// `AIab`: Item Hero Stat Bonus.
        ITEM_HERO_STAT_BONUS = "AIab";
        /// `AIam`: Agility Mod.
        AGILITY_MOD = "AIam";
        /// `AIan`: Animate Dead.
        ANIMATE_DEAD = "AIan";
        /// `AIas`: Item Attack Speed Bonus.
        ITEM_ATTACK_SPEED_BONUS = "AIas";
        /// `AIat`: Attack Bonus.
        ATTACK_BONUS = "AIat";
        /// `AIbl`: Build Tiny Castle.
        BUILD_TINY_CASTLE = "AIbl";
        /// `AIcb`: Orb of Corruption.
        ORB_OF_CORRUPTION = "AIcb";
        /// `AIcf`: Item Cloak Of Flames.
        ITEM_CLOAK_OF_FLAMES = "AIcf";
        /// `AIco`: Item Command.
        ITEM_COMMAND = "AIco";
        /// `AIct`: Item Change TOD.
        ITEM_CHANGE_TOD = "AIct";
        /// `AIda`: Item Defense Aoe.
        ITEM_DEFENSE_AOE = "AIda";
        /// `AIdc`: Item Dispel Chain.
        ITEM_DISPEL_CHAIN = "AIdc";
        /// `AIdd`: Defend (Item).
        DEFEND_ITEM = "AIdd";
        /// `AIde`: Item Armor Bonus.
        ITEM_ARMOR_BONUS = "AIde";
        /// `AIdi`: Item Dispel Aoe.
        ITEM_DISPEL_AOE = "AIdi";
        /// `AIem`: Experience Mod.
        EXPERIENCE_MOD = "AIem";
        /// `AIfa`: Flare Gun.
        FLARE_GUN = "AIfa";
        /// `AIfb`: Orb of Fire.
        ORB_OF_FIRE = "AIfb";
        /// `AIfd`: Figurine Red Drake.
        FIGURINE_RED_DRAKE = "AIfd";
        /// `AIff`: Figurine Furbolg.
        FIGURINE_FURBOLG = "AIff";
        /// `AIfg`: Cloud of Fog (Item).
        CLOUD_OF_FOG_ITEM = "AIfg";
        /// `AIfh`: Figurine Fel Hound.
        FIGURINE_FEL_HOUND = "AIfh";
        /// `AIfr`: Figurine Rock Golem.
        FIGURINE_ROCK_GOLEM = "AIfr";
        /// `AIfs`: Figurine Skeleton.
        FIGURINE_SKELETON = "AIfs";
        /// `AIfu`: Figurine Doom Guard.
        FIGURINE_DOOM_GUARD = "AIfu";
        /// `AIgl`: Fortification Glyph.
        FORTIFICATION_GLYPH = "AIgl";
        /// `AIgo`: Give Gold.
        GIVE_GOLD = "AIgo";
        /// `AIha`: Item Heal Aoe.
        ITEM_HEAL_AOE = "AIha";
        /// `AIhe`: Item Healing.
        ITEM_HEALING = "AIhe";
        /// `AIil`: Item Illusion.
        ITEM_ILLUSION = "AIil";
        /// `AIim`: Intelligence Mod.
        INTELLIGENCE_MOD = "AIim";
        /// `AIlb`: Orb of Lightning(old).
        ORB_OF_LIGHTNING_OLD = "AIlb";
        /// `AIlm`: Level Mod.
        LEVEL_MOD = "AIlm";
        /// `AIlp`: Lightning Purge.
        LIGHTNING_PURGE = "AIlp";
        /// `AIlu`: Give Lumber.
        GIVE_LUMBER = "AIlu";
        /// `AIma`: Item Mana Regain.
        ITEM_MANA_REGAIN = "AIma";
        /// `AImi`: Item Permanent Life Gain.
        ITEM_PERMANENT_LIFE_GAIN = "AImi";
        /// `AIml`: Item Life Bonus.
        ITEM_LIFE_BONUS = "AIml";
        /// `AImm`: Item Mana Bonus.
        ITEM_MANA_BONUS = "AImm";
        /// `AImo`: Item Monster Lure.
        ITEM_MONSTER_LURE = "AImo";
        /// `AImr`: Item Mana Restore Aoe.
        ITEM_MANA_RESTORE_AOE = "AImr";
        /// `AIms`: Move Speed Bonus.
        MOVE_SPEED_BONUS = "AIms";
        /// `AInv`: Inventory.
        INVENTORY = "AInv";
        /// `AIob`: Orb of Frost.
        ORB_OF_FROST = "AIob";
        /// `AIpb`: Orb of Venom.
        ORB_OF_VENOM = "AIpb";
        /// `AIpm`: Item Place Mine.
        ITEM_PLACE_MINE = "AIpm";
        /// `AIpv`: Item Potion Vampirism.
        ITEM_POTION_VAMPIRISM = "AIpv";
        /// `AIra`: Item Restore Aoe.
        ITEM_RESTORE_AOE = "AIra";
        /// `AIrb`: Rune of Rebirth.
        RUNE_OF_REBIRTH = "AIrb";
        /// `AIrc`: Item Reincarnation.
        ITEM_REINCARNATION = "AIrc";
        /// `AIrd`: Raise Dead (Item).
        RAISE_DEAD_ITEM = "AIrd";
        /// `AIre`: Item Restore.
        ITEM_RESTORE = "AIre";
        /// `AIrg`: Potion of Life Regen.
        POTION_OF_LIFE_REGEN = "AIrg";
        /// `AIrm`: Item Regen Mana.
        ITEM_REGEN_MANA = "AIrm";
        /// `AIrs`: Resurrection.
        RESURRECTION = "AIrs";
        /// `AIrt`: Item Recall.
        ITEM_RECALL = "AIrt";
        /// `AIrv`: Item Reveal Map.
        ITEM_REVEAL_MAP = "AIrv";
        /// `AIsa`: Item Speed Aoe.
        ITEM_SPEED_AOE = "AIsa";
        /// `AIsb`: Orb of Spells.
        ORB_OF_SPELLS = "AIsb";
        /// `AIsh`: Summon Headhunter (item).
        SUMMON_HEADHUNTER_ITEM = "AIsh";
        /// `AIsm`: Strength Mod.
        STRENGTH_MOD = "AIsm";
        /// `AIso`: Soul Trap.
        SOUL_TRAP = "AIso";
        /// `AIsp`: Item Speed.
        ITEM_SPEED = "AIsp";
        /// `AIsr`: Runed Bracers.
        RUNED_BRACERS = "AIsr";
        /// `AIta`: Item Detect Aoe.
        ITEM_DETECT_AOE = "AIta";
        /// `AItb`: Dust of Appearance.
        DUST_OF_APPEARANCE = "AItb";
        /// `AItp`: Item Town Portal.
        ITEM_TOWN_PORTAL = "AItp";
        /// `AIva`: Vampiric attack.
        VAMPIRIC_ATTACK = "AIva";
        /// `AIvi`: Item Temporary Invisibility.
        ITEM_TEMPORARY_INVISIBILITY = "AIvi";
        /// `AIvu`: Item Invul.
        ITEM_INVUL = "AIvu";
        /// `AIwb`: Item Web.
        ITEM_WEB = "AIwb";
        /// `AIxm`: Permanent All + 1.
        PERMANENT_ALL_1 = "AIxm";
        /// `ANab`: Alchemist - Acid Bomb.
        ALCHEMIST_ACID_BOMB = "ANab";
        /// `ANba`: Dark Ranger - Black Arrow.
        DARK_RANGER_BLACK_ARROW = "ANba";
        /// `ANbf`: Brewmaster - Breath of Fire.
        BREWMASTER_BREATH_OF_FIRE = "ANbf";
        /// `ANbs`: Orb of Darkness (Black Arrow).
        ORB_OF_DARKNESS_BLACK_ARROW = "ANbs";
        /// `ANbu`: Build (Neutral).
        BUILD_NEUTRAL = "ANbu";
        /// `ANca`: Pit Lord - Cleaving Attack.
        PIT_LORD_CLEAVING_ATTACK = "ANca";
        /// `ANch`: Dark Ranger - Charm.
        DARK_RANGER_CHARM = "ANch";
        /// `ANcl`: Illidan - Channel.
        ILLIDAN_CHANNEL = "ANcl";
        /// `ANcr`: Alchemist - Chemical Rage.
        ALCHEMIST_CHEMICAL_RAGE = "ANcr";
        /// `ANcs`: Tinkerer - Cluster Rockets (Level 0).
        TINKERER_CLUSTER_ROCKETS_LEVEL_0 = "ANcs";
        /// `ANdb`: Brewmaster - Drunken Brawler.
        BREWMASTER_DRUNKEN_BRAWLER = "ANdb";
        /// `ANdc`: Malganis - Dark Conversion.
        MALGANIS_DARK_CONVERSION = "ANdc";
        /// `ANde`: Tinkerer - Demolish (Level 0).
        TINKERER_DEMOLISH_LEVEL_0 = "ANde";
        /// `ANdh`: Brewmaster - Drunken Haze.
        BREWMASTER_DRUNKEN_HAZE = "ANdh";
        /// `ANdo`: Pit Lord - Doom.
        PIT_LORD_DOOM = "ANdo";
        /// `ANdp`: Dark Portal.
        DARK_PORTAL = "ANdp";
        /// `ANdr`: Dark Ranger - Drain.
        DARK_RANGER_DRAIN = "ANdr";
        /// `ANef`: Brewmaster - Storm, Earth and Fire.
        BREWMASTER_STORM_EARTH_AND_FIRE = "ANef";
        /// `ANeg`: Tinkerer - Engineering Upgrade.
        TINKERER_ENGINEERING_UPGRADE = "ANeg";
        /// `ANfb`: Fire Bolt.
        FIRE_BOLT = "ANfb";
        /// `ANfd`: Finger of Death.
        FINGER_OF_DEATH = "ANfd";
        /// `ANfl`: Sea Witch - Forked Lightning.
        SEA_WITCH_FORKED_LIGHTNING = "ANfl";
        /// `ANfy`: Factory.
        FACTORY = "ANfy";
        /// `ANgl`: Exchange Gold for Lumber.
        EXCHANGE_GOLD_FOR_LUMBER = "ANgl";
        /// `ANhs`: Alchemist - Healing Spray.
        ALCHEMIST_HEALING_SPRAY = "ANhs";
        /// `ANht`: Pit Lord - Howl of Terror.
        PIT_LORD_HOWL_OF_TERROR = "ANht";
        /// `ANia`: Firelord - Incinerate.
        FIRELORD_INCINERATE_ANIA = "ANia";
        /// `ANic`: Firelord - Incinerate.
        FIRELORD_INCINERATE_ANIC = "ANic";
        /// `ANin`: Inferno.
        INFERNO = "ANin";
        /// `ANlg`: Exchange Lumber for Gold.
        EXCHANGE_LUMBER_FOR_GOLD = "ANlg";
        /// `ANlm`: Firelord - Summon Lava Spawn.
        FIRELORD_SUMMON_LAVA_SPAWN = "ANlm";
        /// `ANmo`: Monsoon.
        MONSOON = "ANmo";
        /// `ANms`: Sea Witch - Mana Shield.
        SEA_WITCH_MANA_SHIELD = "ANms";
        /// `ANpa`: Parasite.
        PARASITE = "ANpa";
        /// `ANpi`: Permanent Immolation.
        PERMANENT_IMMOLATION = "ANpi";
        /// `ANpr`: Preservation.
        PRESERVATION = "ANpr";
        /// `ANrc`: Rain of Chaos.
        RAIN_OF_CHAOS = "ANrc";
        /// `ANrf`: Pit Lord - Rain of Fire.
        PIT_LORD_RAIN_OF_FIRE = "ANrf";
        /// `ANrg`: Tinkerer - Robo-Goblin (Level 0).
        TINKERER_ROBO_GOBLIN_LEVEL_0 = "ANrg";
        /// `ANsa`: Sanctuary.
        SANCTUARY = "ANsa";
        /// `ANse`: Spell Shield AOE.
        SPELL_SHIELD_AOE = "ANse";
        /// `ANsg`: Beast Master - Summon Bear.
        BEAST_MASTER_SUMMON_BEAR = "ANsg";
        /// `ANsi`: Dark Ranger - Silence.
        DARK_RANGER_SILENCE = "ANsi";
        /// `ANsl`: Malganis - Soul Preservation.
        MALGANIS_SOUL_PRESERVATION = "ANsl";
        /// `ANso`: Firelord - Soul Burn.
        FIRELORD_SOUL_BURN = "ANso";
        /// `ANsp`: Spy.
        SPY = "ANsp";
        /// `ANsq`: Beast Master - Summon Quilbeast.
        BEAST_MASTER_SUMMON_QUILBEAST = "ANsq";
        /// `ANss`: Spell Shield.
        SPELL_SHIELD = "ANss";
        /// `ANst`: Beast Master - Stampede.
        BEAST_MASTER_STAMPEDE = "ANst";
        /// `ANsu`: Submerge (Myrmidon).
        SUBMERGE_MYRMIDON = "ANsu";
        /// `ANsw`: Beast Master - Summon Hawk.
        BEAST_MASTER_SUMMON_HAWK = "ANsw";
        /// `ANsy`: Tinkerer - Summon Factory (Level 0).
        TINKERER_SUMMON_FACTORY_LEVEL_0 = "ANsy";
        /// `ANth`: Thorny Shield (Creep).
        THORNY_SHIELD_CREEP = "ANth";
        /// `ANtm`: Alchemist - Transmute.
        ALCHEMIST_TRANSMUTE = "ANtm";
        /// `ANto`: Sea Witch - Tornado.
        SEA_WITCH_TORNADO = "ANto";
        /// `ANvc`: Firelord - Volcano.
        FIRELORD_VOLCANO = "ANvc";
        /// `ANwm`: Watery Minion.
        WATERY_MINION = "ANwm";
        /// `AOac`: Command Aura.
        COMMAND_AURA = "AOac";
        /// `AOae`: Chieftain - Endurance Aura.
        CHIEFTAIN_ENDURANCE_AURA = "AOae";
        /// `AObu`: Build (Orc).
        BUILD_ORC = "AObu";
        /// `AOcl`: Farseer - Chain Lightning.
        FARSEER_CHAIN_LIGHTNING = "AOcl";
        /// `AOcr`: Blade Master - Critical Strike.
        BLADE_MASTER_CRITICAL_STRIKE = "AOcr";
        /// `AOeq`: Farseer - Earthquake.
        FARSEER_EARTHQUAKE = "AOeq";
        /// `AOfs`: Farseer - Far Sight.
        FARSEER_FAR_SIGHT = "AOfs";
        /// `AOhw`: Shadow Hunter - Healing Wave.
        SHADOW_HUNTER_HEALING_WAVE = "AOhw";
        /// `AOhx`: Shadow Hunter - Hex.
        SHADOW_HUNTER_HEX = "AOhx";
        /// `AOmi`: Blade Master - Mirror Image.
        BLADE_MASTER_MIRROR_IMAGE = "AOmi";
        /// `AOr2`: Cairne - Endurance Aura.
        CAIRNE_ENDURANCE_AURA = "AOr2";
        /// `AOre`: Chieftain - Reincarnation.
        CHIEFTAIN_REINCARNATION = "AOre";
        /// `AOsf`: Farseer - Spirit Wolf.
        FARSEER_SPIRIT_WOLF = "AOsf";
        /// `AOsh`: Chieftain - Shock Wave.
        CHIEFTAIN_SHOCK_WAVE = "AOsh";
        /// `AOsw`: Shadow Hunter - Serpent Ward.
        SHADOW_HUNTER_SERPENT_WARD_AOSW = "AOsw";
        /// `AOvd`: Shadow Hunter - Voodooo.
        SHADOW_HUNTER_VOODOOO = "AOvd";
        /// `AOwd`: Shadow Hunter - Serpent Ward.
        SHADOW_HUNTER_SERPENT_WARD_AOWD = "AOwd";
        /// `AOwk`: Blade Master - Wind Walk.
        BLADE_MASTER_WIND_WALK = "AOwk";
        /// `AOws`: Chieftain - War Stomp.
        CHIEFTAIN_WAR_STOMP = "AOws";
        /// `AOww`: Blade Master - Bladestorm.
        BLADE_MASTER_BLADESTORM = "AOww";
        /// `ARal`: Rally.
        RALLY = "ARal";
        /// `AUan`: Death Knight - Animate Dead.
        DEATH_KNIGHT_ANIMATE_DEAD = "AUan";
        /// `AUau`: Death Knight - Unholy Aura.
        DEATH_KNIGHT_UNHOLY_AURA = "AUau";
        /// `AUav`: Dreadlord - Vampiric Aura.
        DREADLORD_VAMPIRIC_AURA = "AUav";
        /// `AUbu`: Build (Undead).
        BUILD_UNDEAD = "AUbu";
        /// `AUcb`: Crypt Lord - Carrion Scarabs.
        CRYPT_LORD_CARRION_SCARABS = "AUcb";
        /// `AUcs`: Dreadlord - Carrion Swarm.
        DREADLORD_CARRION_SWARM = "AUcs";
        /// `AUdc`: Death Knight - Death Coil.
        DEATH_KNIGHT_DEATH_COIL = "AUdc";
        /// `AUdd`: Lich - Death and Decay.
        LICH_DEATH_AND_DECAY = "AUdd";
        /// `AUdp`: Death Knight - Death Pact.
        DEATH_KNIGHT_DEATH_PACT = "AUdp";
        /// `AUdr`: Lich - Dark Ritual.
        LICH_DARK_RITUAL = "AUdr";
        /// `AUds`: Dark Summoning.
        DARK_SUMMONING = "AUds";
        /// `AUfa`: Lich - Frost Armor.
        LICH_FROST_ARMOR = "AUfa";
        /// `AUfn`: Lich - Frost Nova.
        LICH_FROST_NOVA = "AUfn";
        /// `AUfu`: Lich - Frost Armor (Autocast).
        LICH_FROST_ARMOR_AUTOCAST = "AUfu";
        /// `AUim`: Crypt Lord - Impale.
        CRYPT_LORD_IMPALE = "AUim";
        /// `AUin`: Dreadlord - Inferno.
        DREADLORD_INFERNO = "AUin";
        /// `AUls`: Crypt Lord - Locust Swarm.
        CRYPT_LORD_LOCUST_SWARM = "AUls";
        /// `AUnP`: Upgrade In Progress (a structure's while it upgrades; the engine's).
        UPGRADE_IN_PROGRESS = "AUnP";
        /// `AUsl`: Dreadlord - Sleep.
        DREADLORD_SLEEP = "AUsl";
        /// `AUts`: Crypt Lord - Spiked Carapace.
        CRYPT_LORD_SPIKED_CARAPACE = "AUts";
        /// `Aabs`: Absorb Mana.
        ABSORB_MANA = "Aabs";
        /// `Aadm`: Abolish Magic.
        ABOLISH_MAGIC = "Aadm";
        /// `Aaha`: Acolyte Harvest.
        ACOLYTE_HARVEST = "Aaha";
        /// `Aakb`: Aura - War Drums.
        AURA_WAR_DRUMS = "Aakb";
        /// `Aall`: Allied Building.
        ALLIED_BUILDING = "Aall";
        /// `Aalr`: Alarm.
        ALARM = "Aalr";
        /// `Aami`: Item Anti-Magic Shell.
        ITEM_ANTI_MAGIC_SHELL = "Aami";
        /// `Aamk`: Attribute Modifier Skill.
        ATTRIBUTE_MODIFIER_SKILL = "Aamk";
        /// `Aams`: Anti-magic Shield.
        ANTI_MAGIC_SHIELD = "Aams";
        /// `Aapl`: Disease Cloud.
        DISEASE_CLOUD = "Aapl";
        /// `Aarm`: Mana Regeneration Aura.
        MANA_REGENERATION_AURA = "Aarm";
        /// `Aasl`: Aura - Slow.
        AURA_SLOW = "Aasl";
        /// `Aast`: Ancestral Spirit.
        ANCESTRAL_SPIRIT = "Aast";
        /// `Aatk`: Attack.
        ATTACK = "Aatk";
        /// `Aave`: Avenger Form.
        AVENGER_FORM = "Aave";
        /// `Aawa`: Awaken.
        AWAKEN = "Aawa";
        /// `Abar`: Barkskin.
        BARKSKIN = "Abar";
        /// `Abdt`: Burrow Detection (Flyers).
        BURROW_DETECTION_FLYERS = "Abdt";
        /// `Abgm`: Blighted Gold mine.
        BLIGHTED_GOLD_MINE = "Abgm";
        /// `Abli`: Blight Growth.
        BLIGHT_GROWTH = "Abli";
        /// `Ablo`: Bloodlust.
        BLOODLUST = "Ablo";
        /// `Ablp`: Blight Placement.
        BLIGHT_PLACEMENT = "Ablp";
        /// `Ablr`: Regenerate on Blight.
        REGENERATE_ON_BLIGHT = "Ablr";
        /// `Abof`: Balls of Fire.
        BALLS_OF_FIRE = "Abof";
        /// `Abrf`: Bearform.
        BEARFORM = "Abrf";
        /// `Absk`: Beserk.
        BESERK = "Absk";
        /// `Abtl`: Battlestations.
        BATTLESTATIONS = "Abtl";
        /// `Abun`: Cargo Hold (Burrow).
        CARGO_HOLD_BURROW = "Abun";
        /// `Abur`: Burrow.
        BURROW = "Abur";
        /// `Acan`: Cannibalize.
        CANNIBALIZE = "Acan";
        /// `Acar`: Cargo Hold.
        CARGO_HOLD_ACAR = "Acar";
        /// `Acha`: Chaos.
        CHAOS = "Acha";
        /// `Achd`: Cargo Hold Death.
        CARGO_HOLD_DEATH = "Achd";
        /// `Achl`: Chaos Cargo Load.
        CHAOS_CARGO_LOAD = "Achl";
        /// `Aclf`: Cloud of Fog.
        CLOUD_OF_FOG = "Aclf";
        /// `Acmg`: Control Magic.
        CONTROL_MAGIC = "Acmg";
        /// `Aco2`: Couple Instant (Archer).
        COUPLE_INSTANT_ARCHER_ACO2 = "Aco2";
        /// `Aco3`: Couple Instant (Hippogryph).
        COUPLE_INSTANT_HIPPOGRYPH = "Aco3";
        /// `Acoa`: Couple (Archer).
        COUPLE_ARCHER = "Acoa";
        /// `Acoh`: Couple (Hippogryph).
        COUPLE_HIPPOGRYPH = "Acoh";
        /// `Acoi`: Couple Instant (Archer).
        COUPLE_INSTANT_ARCHER_ACOI = "Acoi";
        /// `Acpf`: Corporeal Form.
        CORPOREAL_FORM = "Acpf";
        /// `Acri`: Cripple.
        CRIPPLE = "Acri";
        /// `Acor`: Corrosive Breath.
        CORROSIVE_BREATH = "Acor";
        /// `Acrs`: Curse.
        CURSE = "Acrs";
        /// `Acyc`: Cyclone.
        CYCLONE = "Acyc";
        /// `Adch`: Disenchant(old).
        DISENCHANT_OLD = "Adch";
        /// `Adda`: Death Damage (sapper).
        DEATH_DAMAGE_SAPPER = "Adda";
        /// `Adec`: Decouple.
        DECOUPLE = "Adec";
        /// `Adef`: Defend.
        DEFEND = "Adef";
        /// `Adet`: Detector.
        DETECTOR = "Adet";
        /// `Adev`: Devour.
        DEVOUR = "Adev";
        /// `Adis`: Dispel Magic.
        DISPEL_MAGIC = "Adis";
        /// `Adro`: Drop.
        DROP = "Adro";
        /// `Adri`: Drop Instant.
        DROP_INSTANT = "Adri";
        /// `Adta`: Reveal.
        REVEAL = "Adta";
        /// `Adtn`: Detonate.
        DETONATE = "Adtn";
        /// `Adts`: Detect (Magic Sentinel).
        DETECT_MAGIC_SENTINEL = "Adts";
        /// `Advc`: Cargo Hold (Devour).
        CARGO_HOLD_DEVOUR = "Advc";
        /// `Advm`: Devour Magic.
        DEVOUR_MAGIC = "Advm";
        /// `Aeat`: Eat Tree.
        EAT_TREE = "Aeat";
        /// `Aegm`: Entangled Gold Mine.
        ENTANGLED_GOLD_MINE = "Aegm";
        /// `Aenc`: Cargo Hold (Gold Mine).
        CARGO_HOLD_GOLD_MINE = "Aenc";
        /// `Aens`: Ensnare.
        ENSNARE = "Aens";
        /// `Aent`: Entangle.
        ENTANGLE = "Aent";
        /// `Aesn`: Sentinel.
        SENTINEL = "Aesn";
        /// `Aesr`: Sentinel (no research).
        SENTINEL_NO_RESEARCH = "Aesr";
        /// `Aetf`: Ethereal Form.
        ETHEREAL_FORM = "Aetf";
        /// `Aeth`: Ghost (Visible).
        GHOST_VISIBLE = "Aeth";
        /// `Aetl`: Ethereal.
        ETHEREAL = "Aetl";
        /// `Aexh`: Exhume.
        EXHUME = "Aexh";
        /// `Aeye`: Sentry Ward.
        SENTRY_WARD = "Aeye";
        /// `Afae`: Faerie Fire.
        FAERIE_FIRE = "Afae";
        /// `Afak`: Orb of Annihilation.
        ORB_OF_ANNIHILATION = "Afak";
        /// `Afbk`: Feedback.
        FEEDBACK = "Afbk";
        /// `Afih`: On Fire (Human).
        ON_FIRE_HUMAN = "Afih";
        /// `Afin`: On Fire (Night Elf).
        ON_FIRE_NIGHT_ELF = "Afin";
        /// `Afio`: On Fire (Orc).
        ON_FIRE_ORC = "Afio";
        /// `Afiu`: On Fire (Undead).
        ON_FIRE_UNDEAD = "Afiu";
        /// `Afla`: Flare.
        FLARE = "Afla";
        /// `Aflk`: Flak Cannon.
        FLAK_CANNON = "Aflk";
        /// `Afra`: Frost Attack.
        FROST_ATTACK = "Afra";
        /// `Afrb`: Frost Breath.
        FROST_BREATH = "Afrb";
        /// `Afrz`: Freezing Breath.
        FREEZING_BREATH = "Afrz";
        /// `Afsh`: Frag Shards.
        FRAG_SHARDS = "Afsh";
        /// `Afzy`: Frenzy.
        FRENZY = "Afzy";
        /// `Agho`: Ghost.
        GHOST = "Agho";
        /// `Agld`: Gold Mine.
        GOLD_MINE = "Agld";
        /// `Agra`: Grab Tree.
        GRAB_TREE = "Agra";
        /// `Agyb`: Gyrocopter Bombs.
        GYROCOPTER_BOMBS = "Agyb";
        /// `Agyd`: Graveyard.
        GRAVEYARD = "Agyd";
        /// `Agyv`: Detect (Gyrocopter).
        DETECT_GYROCOPTER = "Agyv";
        /// `Ahar`: Harvest.
        HARVEST = "Ahar";
        /// `Ahea`: Heal.
        HEAL = "Ahea";
        /// `Ahid`: Shadow Meld (Akama).
        SHADOW_MELD_AKAMA = "Ahid";
        /// `Ahrl`: Harvest Lumber.
        HARVEST_LUMBER = "Ahrl";
        /// `Ahwd`: Healing Ward.
        HEALING_WARD = "Ahwd";
        /// `Aimp`: Impaling Bolt.
        IMPALING_BOLT = "Aimp";
        /// `Ainf`: Inner Fire.
        INNER_FIRE = "Ainf";
        /// `Aivs`: Invisibility.
        INVISIBILITY = "Aivs";
        /// `Alam`: Sacrifice (Acolyte).
        SACRIFICE_ACOLYTE = "Alam";
        /// `Aliq`: Liquid Fire.
        LIQUID_FIRE = "Aliq";
        /// `Alit`: Lightning Attack.
        LIGHTNING_ATTACK = "Alit";
        /// `Aloa`: Load.
        LOAD = "Aloa";
        /// `Aloc`: Locust.
        LOCUST = "Aloc";
        /// `Alsh`: Lightning Shield.
        LIGHTNING_SHIELD = "Alsh";
        /// `Amai`: Make Items.
        MAKE_ITEMS = "Amai";
        /// `Ambt`: Mana Battery.
        MANA_BATTERY = "Ambt";
        /// `Amdf`: Magic Defense.
        MAGIC_DEFENSE = "Amdf";
        /// `Amec`: Mechanical Critter.
        MECHANICAL_CRITTER = "Amec";
        /// `Amed`: Meat Drop.
        MEAT_DROP = "Amed";
        /// `Amel`: Meat Load.
        MEAT_LOAD = "Amel";
        /// `Amfl`: Mana Flare.
        MANA_FLARE = "Amfl";
        /// `Amgl`: Moon Glaive.
        MOON_GLAIVE = "Amgl";
        /// `Amic`: Militia Conversion.
        MILITIA_CONVERSION = "Amic";
        /// `Amil`: Militia.
        MILITIA = "Amil";
        /// `Amim`: Magic Immunity.
        MAGIC_IMMUNITY = "Amim";
        /// `Amin`: Mine.
        MINE = "Amin";
        /// `Amls`: Aerial Shackles.
        AERIAL_SHACKLES = "Amls";
        /// `Amov`: Move.
        MOVE = "Amov";
        /// `Amtc`: Cargo Hold.
        CARGO_HOLD_AMTC = "Amtc";
        /// `Andt`: Neutral Detection (Reveal ability).
        NEUTRAL_DETECTION_REVEAL_ABILITY = "Andt";
        /// `Ane2`: Neutral Building (any unit).
        NEUTRAL_BUILDING_ANY_UNIT = "Ane2";
        /// `Anei`: Select User.
        SELECT_USER = "Anei";
        /// `Aneu`: Neutral Building.
        NEUTRAL_BUILDING = "Aneu";
        /// `Anhe`: Heal (Creep Normal).
        HEAL_CREEP_NORMAL = "Anhe";
        /// `Aoar`: Aura - Regeneration (Ward).
        AURA_REGENERATION_WARD = "Aoar";
        /// `Aphx`: Phoenix.
        PHOENIX = "Aphx";
        /// `Apig`: Permanent Immolation (graphic).
        PERMANENT_IMMOLATION_GRAPHIC = "Apig";
        /// `Apit`: Purchase Item.
        PURCHASE_ITEM = "Apit";
        /// `Apiv`: Permanent Invisibility.
        PERMANENT_INVISIBILITY = "Apiv";
        /// `Aply`: Polymorph.
        POLYMORPH = "Aply";
        /// `Apo2`: Orb of Venom (Poison Attack).
        ORB_OF_VENOM_POISON_ATTACK = "Apo2";
        /// `Apoi`: Poison Attack.
        POISON_ATTACK = "Apoi";
        /// `Apos`: Possession.
        POSSESSION = "Apos";
        /// `Aprg`: Purge.
        PURGE = "Aprg";
        /// `Aps2`: Possession (Channeling).
        POSSESSION_CHANNELING = "Aps2";
        /// `Apsh`: Phase Shift.
        PHASE_SHIFT = "Apsh";
        /// `Apts`: Plague Toss.
        PLAGUE_TOSS = "Apts";
        /// `Apxf`: Phoenix Fire.
        PHOENIX_FIRE = "Apxf";
        /// `Aque`: Queue.
        QUEUE = "Aque";
        /// `Arai`: Raise Dead.
        RAISE_DEAD = "Arai";
        /// `Arav`: Raven Form (Druid).
        RAVEN_FORM_DRUID = "Arav";
        /// `Arbr`: Reinforced Burrows.
        REINFORCED_BURROWS = "Arbr";
        /// `Arej`: Rejuvination.
        REJUVINATION = "Arej";
        /// `Arel`: Regen Life.
        REGEN_LIFE = "Arel";
        /// `Aren`: Renew.
        RENEW = "Aren";
        /// `Arep`: Repair (Orc).
        REPAIR_ORC = "Arep";
        /// `Aret`: Retrain.
        RETRAIN = "Aret";
        /// `Arng`: Revenge.
        REVENGE = "Arng";
        /// `Aroa`: Roar.
        ROAR = "Aroa";
        /// `Aroc`: Rocket Attack.
        ROCKET_ATTACK = "Aroc";
        /// `Aroo`: Root.
        ROOT = "Aroo";
        /// `Arpb`: Replenish (Life & Mana).
        REPLENISH_LIFE_AND_MANA = "Arpb";
        /// `Arpl`: Replenish (Life).
        REPLENISH_LIFE = "Arpl";
        /// `Arpm`: Replenish (Mana).
        REPLENISH_MANA = "Arpm";
        /// `Arsk`: Resistant Skin.
        RESISTANT_SKIN = "Arsk";
        /// `Arst`: Restoration.
        RESTORATION = "Arst";
        /// `Artn`: Return.
        RETURN = "Artn";
        /// `Asac`: Sacrifice (Sacrificial Pit).
        SACRIFICE_SACRIFICIAL_PIT = "Asac";
        /// `Asal`: Pillage.
        PILLAGE = "Asal";
        /// `Asds`: Self Destruct.
        SELF_DESTRUCT = "Asds";
        /// `Asei`: Sell Items.
        SELL_ITEMS = "Asei";
        /// `Asel`: Sell Units.
        SELL_UNITS = "Asel";
        /// `Ashm`: Shadow Meld.
        SHADOW_MELD = "Ashm";
        /// `Ashs`: Shadow Sight.
        SHADOW_SIGHT = "Ashs";
        /// `Asid`: Sell Item (a Marketplace's goods, chosen as the game goes).
        SELL_ITEM = "Asid";
        /// `Asud`: Sell Unit (as Sell Item, units).
        SELL_UNIT = "Asud";
        /// `Aslo`: Slow.
        SLOW = "Aslo";
        /// `Aspb`: Spell Book.
        /// `Aspa`: Spider Attack (the Crypt Fiend's).
        SPIDER_ATTACK = "Aspa";
        SPELL_BOOK = "Aspb";
        /// `Aspd`: Spawn Spider.
        SPAWN_SPIDER = "Aspd";
        /// `Asph`: Sphere.
        SPHERE = "Asph";
        /// `Aspi`: Spiked Barricades.
        SPIKED_BARRICADES = "Aspi";
        /// `Aspl`: Spirit Link.
        SPIRIT_LINK = "Aspl";
        /// `Aspo`: Slow Poison.
        SLOW_POISON = "Aspo";
        /// `Asps`: Spell Steal.
        SPELL_STEAL = "Asps";
        /// `Assk`: Hardened Skin.
        HARDENED_SKIN = "Assk";
        /// `Assp`: Spawn Spiderling.
        SPAWN_SPIDERLING = "Assp";
        /// `Asta`: Stasis Trap.
        STASIS_TRAP = "Asta";
        /// `Astd`: Stand Down.
        STAND_DOWN = "Astd";
        /// `Aste`: Mana Steal.
        MANA_STEAL = "Aste";
        /// `Asth`: Storm Hammers.
        STORM_HAMMERS = "Asth";
        /// `Astn`: Stone Form.
        STONE_FORM = "Astn";
        /// `Atau`: Taunt.
        TAUNT = "Atau";
        /// `Atdg`: Tornado Damage.
        TORNADO_DAMAGE = "Atdg";
        /// `Atlp`: Load Pilot.
        LOAD_PILOT = "Atlp";
        /// `Atol`: Tree of life (for attaching art).
        TREE_OF_LIFE_FOR_ATTACHING_ART = "Atol";
        /// `Atpi`: Tank Pilot.
        TANK_PILOT = "Atpi";
        /// `Atru`: Detect (Shade).
        DETECT_SHADE = "Atru";
        /// `Atsp`: Tornado Spin.
        TORNADO_SPIN = "Atsp";
        /// `Atwa`: Tornado Wander.
        TORNADO_WANDER = "Atwa";
        /// `Auco`: Unstable Concoction.
        UNSTABLE_CONCOCTION = "Auco";
        /// `Auhf`: Unholy Frenzy.
        UNHOLY_FRENZY = "Auhf";
        /// `Ault`: Ultravision.
        ULTRAVISION = "Ault";
        /// `Auns`: Unsummon.
        UNSUMMON = "Auns";
        /// `Aupg`: Upgrade Structure.
        UPGRADE_STRUCTURE = "Aupg";
        /// `Aven`: Venom Spears.
        VENOM_SPEARS = "Aven";
        /// `Avng`: Vengeance.
        VENGEANCE = "Avng";
        /// `Avul`: Invulnerable.
        INVULNERABLE = "Avul";
        /// `Awan`: Wander.
        WANDER = "Awan";
        /// `Awar`: Pulverize.
        PULVERIZE = "Awar";
        /// `Aweb`: Web.
        WEB = "Aweb";
        /// `Awha`: Wisp Harvest.
        WISP_HARVEST = "Awha";
        /// `Awrp`: Warp.
        WARP = "Awrp";
        // The engine's own ability classes' codes (their vf7), named by class.
        /// `AAat`: CAbilityAutoTargetSpell (the engine's own class, no data ability).
        AUTO_TARGET_SPELL = "AAat";
        /// `AAbt`: CAbilityButton (the engine's own class, no data ability).
        BUTTON = "AAbt";
        /// `AAcs`: CAbilityClosestTargetSpell (the engine's own class, no data ability).
        CLOSEST_TARGET_SPELL = "AAcs";
        /// `AAin`: CAbilityInterfaced (the engine's own class, no data ability).
        INTERFACED = "AAin";
        /// `AAms`: CAbilityModalSpell (the engine's own class, no data ability).
        MODAL_SPELL = "AAms";
        /// `AAns`: CAbilityNeutralSpell (the engine's own class, no data ability).
        NEUTRAL_SPELL = "AAns";
        /// `AAsm`: CAbilitySimpleSpell (the engine's own class, no data ability).
        SIMPLE_SPELL = "AAsm";
        /// `AAsp`: CAbilitySpell (the engine's own class, no data ability).
        SPELL = "AAsp";
        /// `ABbs`: CAbilityBaseBuild (the engine's own class, no data ability).
        BASE_BUILD = "ABbs";
        /// `ABon`: CBonusBase (the engine's own class, no data ability).
        BONUS_BASE = "ABon";
        /// `ABqu`: CAbilityBuildQueue (the engine's own class, no data ability).
        BUILD_QUEUE = "ABqu";
        /// `AEpa`: CAbilityPoisonArrows (the engine's own class, no data ability).
        POISON_ARROWS = "AEpa";
        /// `AEsd`: CAbilityStarfallDrain (the engine's own class, no data ability).
        STARFALL_DRAIN = "AEsd";
        /// `AEtr`: CAbilityTranquilityRegen (the engine's own class, no data ability).
        TRANQUILITY_REGEN = "AEtr";
        /// `AHRa`: CAbilityRangerArrow (the engine's own class, no data ability).
        RANGER_ARROW = "AHRa";
        /// `AIaa`: CAbilityAttackMod (the engine's own class, no data ability).
        ATTACK_MOD = "AIaa";
        /// `AIca`: CAbilityItemCureAoe (the engine's own class, no data ability).
        ITEM_CURE_AOE = "AIca";
        /// `AIdm`: CAbilityItemDamageAoe (the engine's own class, no data ability).
        ITEM_DAMAGE_AOE = "AIdm";
        /// `AIfc`: CAbilityItemFlyingCarpet (the engine's own class, no data ability).
        ITEM_FLYING_CARPET = "AIfc";
        /// `AIfe`: CAbilityFlagUndead (the engine's own class, no data ability).
        FLAG_UNDEAD = "AIfe";
        /// `AIfl`: CAbilityFlag (the engine's own class, no data ability).
        FLAG = "AIfl";
        /// `AIfm`: CAbilityFlagHuman (the engine's own class, no data ability).
        FLAG_HUMAN = "AIfm";
        /// `AIfn`: CAbilityFlagNightElf (the engine's own class, no data ability).
        FLAG_NIGHT_ELF = "AIfn";
        /// `AIfo`: CAbilityFlagOrc (the engine's own class, no data ability).
        FLAG_ORC = "AIfo";
        /// `AIgp`: CGuardPost (the engine's own class, no data ability).
        GUARD_POST = "AIgp";
        /// `AImn`: CAbilityMaxManaMod (the engine's own class, no data ability).
        MAX_MANA_MOD = "AImn";
        /// `AIpi`: CAbilityItemPermInvis (the engine's own class, no data ability).
        ITEM_PERM_INVIS = "AIpi";
        /// `AIri`: CAbilityItemRandomItem (the engine's own class, no data ability).
        ITEM_RANDOM_ITEM = "AIri";
        /// `AIsi`: CAbilitySightBonus (the engine's own class, no data ability).
        SIGHT_BONUS = "AIsi";
        /// `AIte`: CAbilityItemTeleport (the engine's own class, no data ability).
        ITEM_TELEPORT = "AIte";
        /// `AItm`: CAbilityTome (the engine's own class, no data ability).
        TOME = "AItm";
        /// `ANbr`: CAbilityBattleRoar (the engine's own class, no data ability).
        BATTLE_ROAR_CLASS = "ANbr";
        /// `ANmd`: CAbilityMonsoonDrain (the engine's own class, no data ability).
        MONSOON_DRAIN = "ANmd";
        /// `ANmr`: CAbilityMindRot (the engine's own class, no data ability).
        MIND_ROT_CLASS = "ANmr";
        /// `ANre`: CAbilityNeutralRegen (the engine's own class, no data ability).
        NEUTRAL_REGEN = "ANre";
        /// `ANrl`: CAbilityNeutralRegenLife (the engine's own class, no data ability).
        NEUTRAL_REGEN_LIFE = "ANrl";
        /// `ANrm`: CAbilityNeutralRegenMana (the engine's own class, no data ability).
        NEUTRAL_REGEN_MANA = "ANrm";
        /// `APas`: CAbilityPassive (the engine's own class, no data ability).
        PASSIVE = "APas";
        /// `APbo`: CAbilityPersistentBonus (the engine's own class, no data ability).
        PERSISTENT_BONUS = "APbo";
        /// `AREB`: CAbilityRegenBase (the engine's own class, no data ability).
        REGEN_BASE = "AREB";
        /// `ARes`: CAbilityResearch (the engine's own class, no data ability).
        RESEARCH = "ARes";
        /// `ASnP`: CAbilitySacrificeInProgress (the engine's own class, no data ability).
        SACRIFICE_IN_PROGRESS = "ASnP";
        /// `ATrn`: CAbilityTrain (the engine's own class, no data ability).
        TRAIN = "ATrn";
        /// `Aabr`: CAbilityAuraBlightRegen (the engine's own class, no data ability).
        AURA_BLIGHT_REGEN = "Aabr";
        /// `Aasp`: CAbilityAuraSpell (the engine's own class, no data ability).
        AURA_SPELL = "Aasp";
        /// `Abou`: CAbilityBounce (the engine's own class, no data ability).
        BOUNCE = "Abou";
        /// `Acal`: CAbilityCallToArms (the engine's own class, no data ability).
        CALL_TO_ARMS = "Acal";
        /// `Achb`: CAbilityChaosBase (the engine's own class, no data ability).
        CHAOS_BASE = "Achb";
        /// `Acou`: CAbilityCouple (the engine's own class, no data ability).
        COUPLE = "Acou";
        /// `Acrg`: CAbilityCargo (the engine's own class, no data ability).
        CARGO = "Acrg";
        /// `Admg`: CAbilityNotifyDamage (the engine's own class, no data ability).
        NOTIFY_DAMAGE = "Admg";
        /// `Afbs`: COnFireBase (the engine's own class, no data ability).
        ON_FIRE_BASE = "Afbs";
        /// `Afir`: CAbilityOnFire (the engine's own class, no data ability).
        ON_FIRE = "Afir";
        /// `Agmb`: CAbilityGoldMineBase (the engine's own class, no data ability).
        GOLD_MINE_BASE = "Agmb";
        /// `Agyc`: CAbilityGraveyardCorpse (the engine's own class, no data ability).
        GRAVEYARD_CORPSE = "Agyc";
        /// `Alea`: CAbilityLeash (the engine's own class, no data ability).
        LEASH = "Alea";
        /// `Amor`: CAbilityMorph (the engine's own class, no data ability).
        MORPH = "Amor";
        /// `Amou`: CAbilityMount (the engine's own class, no data ability).
        MOUNT = "Amou";
        /// `Anit`: CAbilityNightVision (the engine's own class, no data ability).
        NIGHT_VISION = "Anit";
        /// `Arem`: CAbilityRegenMana (the engine's own class, no data ability).
        REGEN_MANA = "Arem";
        /// `Arsc`: CAbilityRescuable (the engine's own class, no data ability).
        RESCUABLE = "Arsc";
        /// `Asla`: CAbilitySleepAlways (the engine's own class, no data ability).
        SLEEP_ALWAYS = "Asla";
        /// `Asod`: CAbilityDeathSpawn (the engine's own class, no data ability).
        DEATH_SPAWN = "Asod";
        /// `Asou`: CAbilitySoulTrapped (the engine's own class, no data ability).
        SOUL_TRAPPED = "Asou";
        /// `Atdp`: CAbilityTankDropPilot (the engine's own class, no data ability).
        TANK_DROP_PILOT = "Atdp";
        /// `Atrg`: CAbilityTarget (the engine's own class, no data ability).
        TARGET_CLASS = "Atrg";
        /// `Attu`: CAbilityTankTurret (the engine's own class, no data ability).
        TANK_TURRET = "Attu";
    }
}

/// Buffs (AbilityBuffData; a few only the engine's).
pub mod buff {
    codes! {
        /// `BAOE`: Aoe.
        AOE = "BAOE";
        /// `BAUR`: Aura.
        AURA = "BAUR";
        /// `BBAR`: Progress Bar.
        PROGRESS_BAR = "BBAR";
        /// `BCbf`: Breath of Frost.
        BREATH_OF_FROST = "BCbf";
        /// `BCtc`: Slam.
        SLAM = "BCtc";
        /// `BDoT`: Dot.
        DOT = "BDoT";
        /// `BEah`: Thorns Aura.
        THORNS_AURA = "BEah";
        /// `BUts`: Thorny Shield.
        THORNY_SHIELD = "BUts";
        /// `BEar`: Trueshot Aura.
        TRUESHOT_AURA = "BEar";
        /// `BEer`: Entangling Roots.
        ENTANGLING_ROOTS = "BEer";
        /// `BEfn`: Force of Nature.
        FORCE_OF_NATURE = "BEfn";
        /// `BEia`: Immolation Aoe.
        IMMOLATION_AOE = "BEia";
        /// `BEim`: Immolation.
        IMMOLATION = "BEim";
        /// `BEme`: Metamorphosis.
        METAMORPHOSIS = "BEme";
        /// `BEsh`: Shadow Strike.
        SHADOW_STRIKE = "BEsh";
        /// `BEst`: Scout.
        SCOUT = "BEst";
        /// `BEsv`: Vengeance.
        VENGEANCE = "BEsv";
        /// `BFig`: Summoned Unit.
        SUMMONED_UNIT = "BFig";
        /// `BHab`: Brilliance Aura.
        BRILLIANCE_AURA = "BHab";
        /// `BHad`: Devotion Aura.
        DEVOTION_AURA = "BHad";
        /// `BHav`: Avatar.
        AVATAR = "BHav";
        /// `BHbd`: Blizzard.
        BLIZZARD = "BHbd";
        /// `BHbn`: Banish.
        BANISH = "BHbn";
        /// `BHbz`: Blizzard Aoe.
        BLIZZARD_AOE = "BHbz";
        /// `BHca`: Cold Arrows.
        COLD_ARROWS_BHCA = "BHca";
        /// `BHds`: Divine Shield.
        DIVINE_SHIELD = "BHds";
        /// `BHfa`: Flame Strike Aoe.
        FLAME_STRIKE_AOE = "BHfa";
        /// `BHfs`: Flame Strike.
        FLAME_STRIKE = "BHfs";
        /// `BHtc`: Thunder Clap.
        THUNDER_CLAP = "BHtc";
        /// `BHwe`: Water Elemental.
        WATER_ELEMENTAL = "BHwe";
        /// `BINV`: BINV.
        BINV = "BINV";
        /// `BIcb`: Corruption.
        CORRUPTION = "BIcb";
        /// `BIcf`: Cloak of Flames.
        CLOAK_OF_FLAMES = "BIcf";
        /// `BIil`: Illusion.
        ILLUSION = "BIil";
        /// `BImo`: Monster Lure.
        MONSTER_LURE = "BImo";
        /// `BIpb`: Venom.
        VENOM_BIPB = "BIpb";
        /// `BIpd`: Venom.
        VENOM_BIPD = "BIpd";
        /// `BIpi`: Poison Damage Stack Info.
        POISON_DAMAGE_STACK_INFO = "BIpi";
        /// `BIpv`: Vampiric Potion.
        VAMPIRIC_POTION = "BIpv";
        /// `BIrb`: Reborn.
        REBORN = "BIrb";
        /// `BIrg`: Rejuvenation.
        REJUVENATION_BIRG = "BIrg";
        /// `BIrl`: Regeneration.
        REGENERATION = "BIrl";
        /// `BIrm`: Clarity Potion.
        CLARITY_POTION = "BIrm";
        /// `BIsh`: Headhunter Spirit.
        HEADHUNTER_SPIRIT = "BIsh";
        /// `BIsv`: Soul Theft.
        SOUL_THEFT = "BIsv";
        /// `BIwb`: Item Web.
        ITEM_WEB = "BIwb";
        /// `BMDR`: Mana Drain.
        MANA_DRAIN = "BMDR";
        /// `BNab`: Acid Bomb.
        ACID_BOMB = "BNab";
        /// `BNba`: Black Arrow.
        BLACK_ARROW = "BNba";
        /// `BNbf`: Breath of Fire.
        BREATH_OF_FIRE = "BNbf";
        /// `BNbr`: Battle Roar.
        BATTLE_ROAR = "BNbr";
        /// `BNcg`: Clockwerk Goblin.
        CLOCKWERK_GOBLIN = "BNcg";
        /// `BNcr`: Chemical Rage.
        CHEMICAL_RAGE = "BNcr";
        /// `BNcs`: Cluster Rockets.
        CLUSTER_ROCKETS = "BNcs";
        /// `BNdc`: Dark Conversion.
        DARK_CONVERSION = "BNdc";
        /// `BNdh`: Drunken Haze.
        DRUNKEN_HAZE = "BNdh";
        /// `BNdi`: Doom.
        DOOM_BNDI = "BNdi";
        /// `BNdm`: Dark Minion.
        DARK_MINION = "BNdm";
        /// `BNdo`: Doom.
        DOOM_BNDO = "BNdo";
        /// `BNef`: Pandaren Elemental.
        PANDAREN_ELEMENTAL = "BNef";
        /// `BNfy`: Pocket Factory.
        POCKET_FACTORY = "BNfy";
        /// `BNhs`: Healing Spray.
        HEALING_SPRAY = "BNhs";
        /// `BNht`: Howl of Terror.
        HOWL_OF_TERROR = "BNht";
        /// `BNic`: Incinerate.
        INCINERATE = "BNic";
        /// `BNin`: Infernal.
        INFERNAL = "BNin";
        /// `BNlm`: Lava Spawn.
        LAVA_SPAWN = "BNlm";
        /// `BNmr`: Mind Rot.
        MIND_ROT = "BNmr";
        /// `BNms`: Mana Shield.
        MANA_SHIELD = "BNms";
        /// `BNpa`: Parasite.
        PARASITE_BNPA = "BNpa";
        /// `BNpi`: Perm Immolation.
        PERM_IMMOLATION = "BNpi";
        /// `BNpm`: Parasite.
        PARASITE_BNPM = "BNpm";
        /// `BNrd`: Rain of Fire.
        RAIN_OF_FIRE = "BNrd";
        /// `BNrf`: Rain Of Fire Aoe.
        RAIN_OF_FIRE_AOE = "BNrf";
        /// `BNsa`: Sanctuary.
        SANCTUARY = "BNsa";
        /// `BNsg`: Bear.
        BEAR = "BNsg";
        /// `BNsi`: Silence.
        SILENCE = "BNsi";
        /// `BNsl`: Soul Preservation.
        SOUL_PRESERVATION = "BNsl";
        /// `BNso`: Soul Burn.
        SOUL_BURN = "BNso";
        /// `BNsq`: Quilbeast.
        QUILBEAST = "BNsq";
        /// `BNss`: Spell Shield.
        SPELL_SHIELD = "BNss";
        /// `BNst`: Stampede.
        STAMPEDE = "BNst";
        /// `BNsw`: Hawk.
        HAWK = "BNsw";
        /// `BNtm`: Transmute.
        TRANSMUTE = "BNtm";
        /// `BNto`: Tornado.
        TORNADO_BNTO = "BNto";
        /// `BNva`: Volcano AOE.
        VOLCANO_AOE = "BNva";
        /// `BNwm`: Watery Minion.
        WATERY_MINION = "BNwm";
        /// `BOac`: Aura Command.
        AURA_COMMAND = "BOac";
        /// `BOae`: Endurance Aura.
        ENDURANCE_AURA = "BOae";
        /// `BOea`: Earthquake Aoe.
        EARTHQUAKE_AOE = "BOea";
        /// `BOeq`: Earthquake.
        EARTHQUAKE = "BOeq";
        /// `BOhx`: Hex.
        HEX = "BOhx";
        /// `BOmi`: Mirror Image.
        MIRROR_IMAGE = "BOmi";
        /// `BOsf`: Feral Spirit.
        FERAL_SPIRIT = "BOsf";
        /// `BOsh`: Shockwave.
        SHOCKWAVE = "BOsh";
        /// `BOvc`: Voodoo Caster.
        VOODOO_CASTER = "BOvc";
        /// `BOvd`: Big Bad Voodoo.
        BIG_BAD_VOODOO = "BOvd";
        /// `BOwd`: Ward.
        WARD = "BOwd";
        /// `BOwk`: Wind Walk.
        WIND_WALK = "BOwk";
        /// `BOww`: Whirlwind Aoe.
        WHIRLWIND_AOE = "BOww";
        /// `BPSE`: Stunned.
        STUNNED_BPSE = "BPSE";
        /// `BSPD`: Speed.
        SPEED = "BSPD";
        /// `BSTI`: Stack Info.
        STACK_INFO = "BSTI";
        /// `BSTN`: Stunned.
        STUNNED_BSTN = "BSTN";
        /// `BTAR`: Target.
        TARGET = "BTAR";
        /// `BTLF`: Timed Life.
        TIMED_LIFE = "BTLF";
        /// `BUan`: Animate Dead.
        ANIMATE_DEAD = "BUan";
        /// `BUau`: Unholy Aura.
        UNHOLY_AURA = "BUau";
        /// `BUav`: Vampiric Aura.
        VAMPIRIC_AURA = "BUav";
        /// `BUcb`: Carrion Beetles.
        CARRION_BEETLES = "BUcb";
        /// `BUcs`: Carrion Swarm.
        CARRION_SWARM = "BUcs";
        /// `BUdd`: Death and Decay.
        DEATH_AND_DECAY = "BUdd";
        /// `BUfa`: Frost Armor.
        FROST_ARMOR = "BUfa";
        /// `BUim`: Impale.
        IMPALE = "BUim";
        /// `BUsl`: Sleep.
        SLEEP = "BUsl";
        /// `BUsp`: Sleep Pause.
        SLEEP_PAUSE = "BUsp";
        /// `BUst`: Sleep Stun.
        SLEEP_STUN = "BUst";
        /// `Babr`: Aura of Blight.
        AURA_OF_BLIGHT = "Babr";
        /// `Bakb`: War Drums.
        WAR_DRUMS = "Bakb";
        /// `Bam2`: Anti-magic Shell.
        ANTI_MAGIC_SHELL_BAM2 = "Bam2";
        /// `Bams`: Anti-magic Shell.
        ANTI_MAGIC_SHELL_BAMS = "Bams";
        /// `Bapl`: Disease.
        DISEASE = "Bapl";
        /// `Barm`: Mana Regeneration Aura.
        MANA_REGENERATION_AURA = "Barm";
        /// `Basl`: Tornado.
        TORNADO_BASL = "Basl";
        /// `Bbar`: Barkskin.
        BARKSKIN = "Bbar";
        /// `Bblo`: Bloodlust.
        BLOODLUST = "Bblo";
        /// `Bbof`: Burning Oil.
        BURNING_OIL = "Bbof";
        /// `Bbsk`: Berserk.
        BERSERK = "Bbsk";
        /// `Bchd`: Dizziness.
        DIZZINESS = "Bchd";
        /// `Bclf`: Cloud.
        CLOUD = "Bclf";
        /// `Bcmg`: Control Magic.
        CONTROL_MAGIC = "Bcmg";
        /// `Bcor`: Corrosive Breath.
        CORROSIVE_BREATH = "Bcor";
        /// `Bcri`: Cripple.
        CRIPPLE = "Bcri";
        /// `Bcrs`: Curse.
        CURSE = "Bcrs";
        /// `Bcsd`: Cold Arrows.
        COLD_ARROWS_BCSD = "Bcsd";
        /// `Bcsi`: Cold Arrow Stack Info.
        COLD_ARROW_STACK_INFO = "Bcsi";
        /// `Bcy2`: Cyclone.
        CYCLONE_BCY2 = "Bcy2";
        /// `Bcyc`: Cyclone.
        CYCLONE_BCYC = "Bcyc";
        /// `Bdbb`: Drain Life & Mana.
        DRAIN_LIFE_AND_MANA = "Bdbb";
        /// `Bdbl`: Drain Life.
        DRAIN_LIFE = "Bdbl";
        /// `Bdbm`: Siphon Mana.
        SIPHON_MANA = "Bdbm";
        /// `Bdcb`: Drain Caster.
        DRAIN_CASTER = "Bdcb";
        /// `Bdcl`: Drain Caster Life.
        DRAIN_CASTER_LIFE = "Bdcl";
        /// `Bdcm`: Drain Caster Mana.
        DRAIN_CASTER_MANA = "Bdcm";
        /// `Bdef`: Scroll of Protection.
        SCROLL_OF_PROTECTION = "Bdef";
        /// `Bdet`: Dust of Appearance.
        DUST_OF_APPEARANCE = "Bdet";
        /// `Bdig`: Digesting.
        DIGESTING = "Bdig";
        /// `Bdtb`: Drain Target.
        DRAIN_TARGET = "Bdtb";
        /// `Bdtl`: Drain Target Life.
        DRAIN_TARGET_LIFE = "Bdtl";
        /// `Bdtm`: Drain Target Mana.
        DRAIN_TARGET_MANA = "Bdtm";
        /// `Bdvv`: Devour.
        DEVOUR = "Bdvv";
        /// `Beat`: Eat Tree.
        EAT_TREE = "Beat";
        /// `Bena`: Ensnare.
        ENSNARE_BENA = "Bena";
        /// `Beng`: Ensnare.
        ENSNARE_BENG = "Beng";
        /// `Bens`: Ensnare.
        ENSNARE_BENS = "Bens";
        /// `Beth`: Ethereal.
        ETHEREAL = "Beth";
        /// `Beye`: Sentry Ward.
        SENTRY_WARD = "Beye";
        /// `Bfae`: Faerie Fire.
        FAERIE_FIRE = "Bfae";
        /// `Bfre`: Freeze.
        FREEZE = "Bfre";
        /// `Bfro`: Slowed.
        SLOWED = "Bfro";
        /// `Bfrz`: Freezing Breath.
        FREEZING_BREATH = "Bfrz";
        /// `Bfzy`: Frenzy.
        FRENZY = "Bfzy";
        /// `Bgra`: Grab Tree.
        GRAB_TREE = "Bgra";
        /// `Bhea`: Heal.
        HEAL = "Bhea";
        /// `Bhwd`: Healing Ward.
        HEALING_WARD = "Bhwd";
        /// `Binf`: Inner Fire.
        INNER_FIRE = "Binf";
        /// `Binv`: Invisibility.
        INVISIBILITY = "Binv";
        /// `BlDT`: Life Dot.
        LIFE_DOT = "BlDT";
        /// `Bliq`: Liquid Fire.
        LIQUID_FIRE = "Bliq";
        /// `Blsa`: Lightning Shield Aoe.
        LIGHTNING_SHIELD_AOE = "Blsa";
        /// `Blsh`: Lightning Shield.
        LIGHTNING_SHIELD = "Blsh";
        /// `Bmec`: Mechanical Critter.
        MECHANICAL_CRITTER = "Bmec";
        /// `Bmfa`: Mana Flare.
        MANA_FLARE_BMFA = "Bmfa";
        /// `Bmfl`: Mana Flare.
        MANA_FLARE_BMFL = "Bmfl";
        /// `Bmil`: Militia.
        MILITIA = "Bmil";
        /// `Bmlc`: Magic Leash Caster.
        MAGIC_LEASH_CASTER = "Bmlc";
        /// `Bmlt`: Aerial Shackles.
        AERIAL_SHACKLES = "Bmlt";
        /// `Boar`: Healing Ward Aura.
        HEALING_WARD_AURA = "Boar";
        /// `Bphx`: Phoenix.
        PHOENIX = "Bphx";
        /// `Bpig`: Panda Immolation.
        PANDA_IMMOLATION = "Bpig";
        /// `Bplg`: Plague Ward.
        PLAGUE_WARD = "Bplg";
        /// `Bply`: Polymorph.
        POLYMORPH = "Bply";
        /// `Bpoc`: Possession.
        POSSESSION_BPOC = "Bpoc";
        /// `Bpoi`: Poison.
        POISON_BPOI = "Bpoi";
        /// `Bpos`: Possession.
        POSSESSION_BPOS = "Bpos";
        /// `Bprg`: Purge.
        PURGE = "Bprg";
        /// `Bprv`: Pulse Reveal.
        PULSE_REVEAL = "Bprv";
        /// `Bpsd`: Poison.
        POISON_BPSD = "Bpsd";
        /// `Bpsh`: Phase Shift.
        PHASE_SHIFT = "Bpsh";
        /// `Bpsi`: Poison Attack Stack Info.
        POISON_ATTACK_STACK_INFO = "Bpsi";
        /// `Bpxf`: Phoenix Fire.
        PHOENIX_FIRE = "Bpxf";
        /// `Brai`: Skeletal Minion.
        SKELETAL_MINION = "Brai";
        /// `Brej`: Rejuvenation.
        REJUVENATION_BREJ = "Brej";
        /// `Brgl`: Range Listener.
        RANGE_LISTENER = "Brgl";
        /// `Broa`: Roar.
        ROAR = "Broa";
        /// `Brpb`: Replenish.
        REPLENISH = "Brpb";
        /// `Brpl`: Essence of Blight.
        ESSENCE_OF_BLIGHT = "Brpl";
        /// `Brpm`: Spirit Touch.
        SPIRIT_TOUCH = "Brpm";
        /// `Bsha`: Shared Vision.
        SHARED_VISION = "Bsha";
        /// `Bshs`: Wand of Shadowsight.
        WAND_OF_SHADOWSIGHT = "Bshs";
        /// `Bslo`: Slow.
        SLOW = "Bslo";
        /// `Bspa`: Spiderling.
        SPIDERLING = "Bspa";
        /// `Bspe`: Speed Bonus.
        SPEED_BONUS = "Bspe";
        /// `Bspl`: Spirit Link.
        SPIRIT_LINK = "Bspl";
        /// `Bspo`: Slow Poison.
        SLOW_POISON_BSPO = "Bspo";
        /// `Bssd`: Slow Poison.
        SLOW_POISON_BSSD = "Bssd";
        /// `Bssi`: Slow Poison Stack Info.
        SLOW_POISON_STACK_INFO = "Bssi";
        /// `Bstt`: Stasis Trap.
        STASIS_TRAP = "Bstt";
        /// `Btau`: Taunt.
        TAUNT = "Btau";
        /// `Btdg`: Tornado Damage Aoe.
        TORNADO_DAMAGE_AOE = "Btdg";
        /// `Btsa`: Tornado Spin Aoe.
        TORNADO_SPIN_AOE = "Btsa";
        /// `Btsp`: Tornado Spin.
        TORNADO_SPIN = "Btsp";
        /// `Buhf`: Unholy Frenzy.
        UNHOLY_FRENZY = "Buhf";
        /// `Buns`: Unsummon.
        UNSUMMON = "Buns";
        /// `Bvng`: Spirit of Vengeance.
        SPIRIT_OF_VENGEANCE = "Bvng";
        /// `Bvul`: Invulnerable.
        INVULNERABLE = "Bvul";
        /// `Bwea`: Web.
        WEB_BWEA = "Bwea";
        /// `Bweb`: Web.
        WEB_BWEB = "Bweb";
        // The engine's own ability classes' codes (their vf7), named by class.
        /// `BNeg`: CBuffEngineeringUpgrade (the engine's own class, no data ability).
        BUFF_ENGINEERING_UPGRADE = "BNeg";
        /// `Btrv`: CBuffTeleportReveal (the engine's own class, no data ability).
        BUFF_TELEPORT_REVEAL = "Btrv";
    }
}

/// Unit types (and `HERO`, the tech id counting every hero).
pub mod unit {
    codes! {
        /// `Eidm`: Demon Hunter (Demon Form).
        DEMON_HUNTER_DEMON_FORM = "Eidm";
        /// `HERO`: Any Hero.
        ANY_HERO = "HERO";
        /// `Hpal`: Paladin.
        PALADIN = "Hpal";
        /// `edoc`: Druid of the Claw.
        DRUID_OF_THE_CLAW = "edoc";
        /// `etrp`: Ancient Protector.
        ANCIENT_PROTECTOR = "etrp";
        /// `hctw`: Cannon Tower.
        CANNON_TOWER = "hctw";
        /// `hgtw`: Guard Tower.
        GUARD_TOWER = "hgtw";
        /// `hpea`: Peasant.
        PEASANT = "hpea";
        /// `hwtw`: Scout Tower.
        SCOUT_TOWER = "hwtw";
        /// `ngol`: Gold Mine.
        GOLD_MINE = "ngol";
        /// `nzep`: Goblin Zeppelin.
        GOBLIN_ZEPPELIN = "nzep";
        /// `otrb`: Orc Burrow.
        ORC_BURROW = "otrb";
        /// `owtw`: Watch Tower.
        WATCH_TOWER = "owtw";
        /// `ugol`: Haunted Gold Mine.
        HAUNTED_GOLD_MINE = "ugol";
        /// `unp1`: Halls of the Dead.
        HALLS_OF_THE_DEAD = "unp1";
        /// `unp2`: Black Citadel.
        BLACK_CITADEL = "unp2";
        /// `ushd`: Shade.
        SHADE = "ushd";
        /// `uzg1`: Spirit Tower.
        SPIRIT_TOWER = "uzg1";
        /// `uzig`: Ziggurat.
        ZIGGURAT = "uzig";
    }
}

/// Item types.
pub mod item {
    codes! {
        /// `pghe`: Potion of Greater Healing.
        POTION_OF_GREATER_HEALING = "pghe";
        /// `ratf`: Claws of Attack +15.
        CLAWS_OF_ATTACK_15 = "ratf";
    }
}

/// Upgrade effects (UpgradeData's `effect` fields).
pub mod upgrade {
    codes! {
        /// `rarm`: Armor.
        ARMOR = "rarm";
        /// `rart`: Armor Type.
        ARMOR_TYPE = "rart";
        /// `rasd`: Spill Distance.
        SPILL_DISTANCE = "rasd";
        /// `ratc`: Attack Targets.
        ATTACK_TARGETS = "ratc";
        /// `ratd`: Attack Dice.
        ATTACK_DICE = "ratd";
        /// `ratr`: Attack Range.
        ATTACK_RANGE = "ratr";
        /// `rats`: Attack Speed.
        ATTACK_SPEED = "rats";
        /// `ratx`: Attack Damage.
        ATTACK_DAMAGE = "ratx";
        /// `raud`: Aura Data (an ability's DataA).
        AURA_DATA_AN_ABILITYS_DATAA = "raud";
        /// `rauv`: Ultravision.
        ULTRAVISION = "rauv";
        /// `renw`: Weapons Enabled.
        WEAPONS_ENABLED = "renw";
        /// `rhpo`: Hit Points Percent.
        HIT_POINTS_PERCENT = "rhpo";
        /// `rhpr`: Hit Point Regeneration.
        HIT_POINT_REGENERATION = "rhpr";
        /// `rhpx`: Hit Points.
        HIT_POINTS = "rhpx";
        /// `rlev`: Ability Level.
        ABILITY_LEVEL = "rlev";
        /// `rlum`: Lumber Harvested.
        LUMBER_HARVESTED = "rlum";
        /// `rman`: Mana Percent.
        MANA_PERCENT = "rman";
        /// `rmnr`: Mana Regeneration.
        MANA_REGENERATION = "rmnr";
        /// `rmnx`: Mana.
        MANA = "rmnx";
        /// `rmov`: Move Speed Percent.
        MOVE_SPEED_PERCENT = "rmov";
        /// `rmvx`: Move Speed.
        MOVE_SPEED = "rmvx";
        /// `rrai`: Raise Dead Duration.
        RAISE_DEAD_DURATION = "rrai";
        /// `rsig`: Sight.
        SIGHT = "rsig";
        /// `rspi`: Spikes.
        SPIKES = "rspi";
        /// `rtma`: Maximum Trained.
        MAXIMUM_TRAINED = "rtma";
    }
}

/// The engine's object classes, as its object tables and the saves name them.
pub mod class {
    use super::Code;

    /// `+agl`: An agent: the family of every agent class not otherwise placed.
    pub const AGENT: &Code = b"+agl";
    /// `^adf`: An agent class's definition (CRlAgentDef).
    pub const AGENT_DEFINITION: &Code = b"^adf";
    /// `^pro`: The family of properties (`flg, `pcu, `pos, `rel).
    pub const PROPERTY_FAMILY: &Code = b"^pro";
    /// `^lis`: The family of listeners (`fcl, `pil, `rtl).
    pub const LISTENER_FAMILY: &Code = b"^lis";
    /// `^mod`: The family of modifiers (`rcm).
    pub const MODIFIER_FAMILY: &Code = b"^mod";
    /// ``flg`: A flag property.
    pub const FLAG: &Code = b"`flg";
    /// ``pcu`: A unit's position property (a cluster).
    pub const UNIT_POSITION: &Code = b"`pcu";
    /// ``pos`: A position property.
    pub const POSITION: &Code = b"`pos";
    /// ``rel`: An indexed real property.
    pub const REAL: &Code = b"`rel";
    /// ``fcl`: A flag's listener.
    pub const FLAG_LISTENER: &Code = b"`fcl";
    /// ``pil`: An in-range listener.
    pub const IN_RANGE_LISTENER: &Code = b"`pil";
    /// ``rtl`: A real's threshold listener.
    pub const THRESHOLD_LISTENER: &Code = b"`rtl";
    /// ``rcm`: A real's continuous modifier.
    pub const CONTINUOUS_MODIFIER: &Code = b"`rcm";
    /// `^pma`: A pathing map (or an accelerator grid's).
    pub const PATHING_MAP: &Code = b"^pma";
    /// `^pmr`: A pathing map's region.
    pub const PATHING_REGION: &Code = b"^pmr";
    /// `^ama`: An accelerator grid.
    pub const ACCELERATOR_GRID: &Code = b"^ama";
    /// `l^sy`: The long-range pathing system.
    pub const LONG_RANGE_SYSTEM: &Code = b"l^sy";
    /// `l^ac`: The long-range pathing system's accelerator (its four grids).
    pub const LONG_RANGE_ACCELERATOR: &Code = b"l^ac";
    /// `l^pa`: A path (a walk's).
    pub const PATH: &Code = b"l^pa";
    /// `^clu`: A walking cluster.
    pub const CLUSTER: &Code = b"^clu";
    /// `^clg`: A cluster's goal (provisional name).
    pub const CLUSTER_GOAL: &Code = b"^clg";
    /// `+w3u`: A unit (CUnit).
    pub const UNIT: &Code = b"+w3u";
    /// `+w3d`: A destructable.
    pub const DESTRUCTABLE: &Code = b"+w3d";
    /// `item`: An item.
    pub const ITEM: &Code = b"item";
    /// `+ply`: A player.
    pub const PLAYER: &Code = b"+ply";
    /// `ally`: A player's alliance.
    pub const ALLIANCE: &Code = b"ally";
    /// `+gam`: The game's state (CGameState).
    pub const GAME_STATE: &Code = b"+gam";
    /// `+qum`: The trigger queue.
    pub const TRIGGER_QUEUE: &Code = b"+qum";
    /// `gccm`: The game cache manager.
    pub const GAME_CACHE_MANAGER: &Code = b"gccm";
    /// `gcch`: A game cache (InitGameCache).
    pub const GAME_CACHE: &Code = b"gcch";
    /// `ghth`: A hashtable (InitHashtable).
    pub const HASHTABLE: &Code = b"ghth";
    /// `+trg`: A trigger.
    pub const TRIGGER: &Code = b"+trg";
    /// `+tac`: A trigger's action.
    pub const TRIGGER_ACTION: &Code = b"+tac";
    /// `tcnd`: A trigger's condition (TriggerAddCondition).
    pub const TRIGGER_CONDITION: &Code = b"tcnd";
    /// `+tex`: A trigger's execution.
    pub const TRIGGER_EXECUTION: &Code = b"+tex";
    /// `+flt`: A filter or condition function.
    pub const FILTER: &Code = b"+flt";
    /// `bAnd`: A boolean expression's And.
    pub const BOOLEXPR_AND: &Code = b"bAnd";
    /// `bNot`: A boolean expression's Not.
    pub const BOOLEXPR_NOT: &Code = b"bNot";
    /// `hdlr`: A script function's handler.
    pub const HANDLER: &Code = b"hdlr";
    /// `+tmr`: A timer.
    pub const TIMER: &Code = b"+tmr";
    /// `+tid`: A timer dialog.
    pub const TIMER_DIALOG: &Code = b"+tid";
    /// `+grp`: A group.
    pub const GROUP: &Code = b"+grp";
    /// `+loc`: A location.
    pub const LOCATION: &Code = b"+loc";
    /// `+rct`: A rect.
    pub const RECT: &Code = b"+rct";
    /// `+agr`: A region (CreateRegion).
    pub const REGION: &Code = b"+agr";
    /// `+frc`: A force.
    pub const FORCE: &Code = b"+frc";
    /// `+snd`: A sound.
    pub const SOUND: &Code = b"+snd";
    /// `+fgm`: A fog modifier.
    pub const FOG_MODIFIER: &Code = b"+fgm";
    /// `+fgt`: A fogged attack's reveal of its attacker (`FUN_6f67fc70`; parent `+fgm`).
    pub const FOGGED_ATTACK_REVEAL: &Code = b"+fgt";
    /// `+dlg`: A dialog.
    pub const DIALOG: &Code = b"+dlg";
    /// `+dlb`: A dialog's button.
    pub const DIALOG_BUTTON: &Code = b"+dlb";
    /// `+ldb`: A leaderboard.
    pub const LEADERBOARD: &Code = b"+ldb";
    /// `+mdb`: A multiboard.
    pub const MULTIBOARD: &Code = b"+mdb";
    /// `+que`: A quest.
    pub const QUEST: &Code = b"+que";
    /// `+qui`: A quest's item.
    pub const QUEST_ITEM: &Code = b"+qui";
    /// `+dfc`: A defeat condition.
    pub const DEFEAT_CONDITION: &Code = b"+dfc";
    /// `+trk`: A trackable.
    pub const TRACKABLE: &Code = b"+trk";
    /// `+cst`: A camera setup.
    pub const CAMERA_SETUP: &Code = b"+cst";
    /// `+EIP`: A special effect (AddSpecialEffect).
    pub const SPECIAL_EFFECT: &Code = b"+EIP";
    /// `+EIm`: An effect image (a buff's art; the second table's).
    pub const EFFECT_IMAGE: &Code = b"+EIm";
    /// `+EIf`: A fogged effect image (CEffectImageFogged; an entangling Tree of Life's).
    pub const EFFECT_IMAGE_FOGGED: &Code = b"+EIf";
    /// `+cam`: The game interface's camera (the second table's).
    pub const CAMERA: &Code = b"+cam";
    /// `+ccu`: The cursor (the second table's).
    pub const CURSOR: &Code = b"+ccu";
    /// `+ccp`: The cursor's other object (the second table's; provisional name).
    pub const CURSOR_PART: &Code = b"+ccp";
    /// `+wgh`: A fog ghost (the second table's).
    pub const FOG_GHOST: &Code = b"+wgh";
    /// `pool`: A unit pool.
    pub const UNIT_POOL: &Code = b"pool";
    /// `ipol`: An item pool.
    pub const ITEM_POOL: &Code = b"ipol";
    /// `uevt`: A unit event.
    pub const UNIT_EVENT: &Code = b"uevt";
    /// `pevt`: A player (or player-unit) event.
    pub const PLAYER_EVENT: &Code = b"pevt";
    /// `tmet`: A timer's expiry event.
    pub const TIMER_EXPIRED_EVENT: &Code = b"tmet";
    /// `wsed`: A timer's expiry event's data (CTimerExpireEventData).
    pub const TIMER_EXPIRED_DATA: &Code = b"wsed";
    /// `tmvt`: A timer event (TriggerRegisterTimerEvent).
    pub const TIMER_EVENT: &Code = b"tmvt";
    /// `gfvt`: A game state event.
    pub const GAME_STATE_EVENT: &Code = b"gfvt";
    /// `gevt`: A game event.
    pub const GAME_EVENT: &Code = b"gevt";
    /// `alvt`: An alliance event.
    pub const ALLIANCE_EVENT: &Code = b"alvt";
    /// `psvt`: A player state event.
    pub const PLAYER_STATE_EVENT: &Code = b"psvt";
    /// `psld`: A player state event's data (CPlayerStateLimitEventData), made as the
    /// event fires (CPlayerStateEventReg vf3 `FUN_6f27cd00`).
    pub const PLAYER_STATE_EVENT_DATA: &Code = b"psld";
    /// `pcvt`: A player chat event.
    pub const PLAYER_CHAT_EVENT: &Code = b"pcvt";
    /// `usvt`: A unit state event.
    pub const UNIT_STATE_EVENT: &Code = b"usvt";
    /// `wdvt`: A widget death event.
    pub const WIDGET_DEATH_EVENT: &Code = b"wdvt";
    /// `vevt`: A variable event.
    pub const VARIABLE_EVENT: &Code = b"vevt";
    /// `devt`: A dialog event.
    pub const DIALOG_EVENT: &Code = b"devt";
    /// `bevt`: A dialog button event.
    pub const DIALOG_BUTTON_EVENT: &Code = b"bevt";
    /// `rgvt`: A unit-in-range event.
    pub const UNIT_IN_RANGE_EVENT: &Code = b"rgvt";
    /// `+rev`: A region's enter or leave event.
    pub const REGION_EVENT: &Code = b"+rev";
    /// `+tev`: A trackable event.
    pub const TRACKABLE_EVENT: &Code = b"+tev";
    /// `+ord`: An order.
    pub const ORDER: &Code = b"+ord";
    /// `ordt`: An order's tasks (provisional name: first of an order's objects).
    pub const ORDER_TASKS: &Code = b"ordt";
    /// `task`: A task.
    pub const TASK: &Code = b"task";
    /// `tskA`: A block (CTaskAction, `FUN_6f6927a0`): sets the unit's state as it runs.
    pub const TASK_A: &Code = b"tskA";
    /// `tskO`: A task of the kind `tskO` (provisional).
    pub const TASK_O: &Code = b"tskO";
    /// `tskt`: A task at a target (CTaskTarget, `FUN_6f6926b0`).
    pub const TASK_T: &Code = b"tskt";
    /// `tsk.`: A task at a point (CTaskPoint, `FUN_6f692120`).
    pub const TASK_POINT: &Code = b"tsk.";
    /// `ptcf`: Made as a trained unit is finished, and freed at once (provisional name).
    pub const TRAINING_DONE: &Code = b"ptcf";
    /// `AI-1`: A player's AI (CCompAI).
    pub const PLAYER_AI: &Code = b"AI-1";
    /// `AI-C`: A computer's captain.
    pub const AI_CAPTAIN: &Code = b"AI-C";
    /// `AI-U`: A unit's AI.
    pub const UNIT_AI: &Code = b"AI-U";
    /// `AItr`: An AI script's thread (CThreadAI, `FUN_6f919123`): its main's, made by
    /// StartMeleeAI, and each StartThread's.
    pub const AI_THREAD: &Code = b"AItr";
    /// `AIts`: A town site (a gold mine's).
    pub const TOWN_SITE: &Code = b"AIts";
    /// `AIcs`: A creep camp.
    pub const CREEP_CAMP: &Code = b"AIcs";
    /// `uref`: A unit AI's place in an AI list.
    pub const UNIT_AI_REF: &Code = b"uref";
    /// `rlyp`: A rally point.
    pub const RALLY_POINT: &Code = b"rlyp";
    /// `shft`: A gold mine's shaft.
    pub const MINE_SHAFT: &Code = b"shft";
    /// `rslt`: A gold mine's work slot.
    pub const MINE_SLOT: &Code = b"rslt";
    /// `spla`: A splash weapon's.
    pub const SPLASH: &Code = b"spla";
    /// `+Wup`: A player's upgrade (CPlayerUpgrade): its levels researched.
    pub const PLAYER_UPGRADE: &Code = b"+Wup";
    /// `Cdet`: A detection (the flyers' burrow detection).
    pub const DETECTION: &Code = b"Cdet";
    /// `Csha`: A unit's shared vision, as buffs show it to players (Ensnare's, Web's,
    /// Faerie Fire's, Devour's, ...).
    pub const SHARED_VISION: &Code = b"Csha";
    /// `Xbli`: A blight growth (a haunted mine's, a structure's).
    pub const BLIGHT_GROWTH: &Code = b"Xbli";
    /// `B-Mi`: A missile weapon's missile (CMissile).
    pub const MISSILE: &Code = b"B-Mi";
    /// `B-Ar`: An artillery weapon's shell (CArtillery).
    pub const ARTILLERY: &Code = b"B-Ar";
    /// `B-Ms`: A splash missile weapon's missile (CMissileSplash).
    pub const MISSILE_SPLASH: &Code = b"B-Ms";
    /// `B-Mb`: A bouncing missile weapon's missile (CMissileBounce).
    pub const MISSILE_BOUNCE: &Code = b"B-Mb";
    /// `B-Ml`: A line missile weapon's missile (CMissileLine).
    pub const MISSILE_LINE: &Code = b"B-Ml";
    /// `B-Al`: A line artillery weapon's shell (CArtilleryLine).
    pub const ARTILLERY_LINE: &Code = b"B-Al";
    /// `Mspo`: Slow Poison's missile (CMissileSlowPoison).
    pub const SLOW_POISON_MISSILE: &Code = b"Mspo";
    /// `Mpoi`: Poison Attack's and Envenomed Spears' missile (CMissilePoisonAttack).
    pub const POISON_ATTACK_MISSILE: &Code = b"Mpoi";
    /// `Mfro`: Frost Attack's and Frost Breath's missile (CMissileFrost).
    pub const FROST_MISSILE: &Code = b"Mfro";
    /// `Mfrb`: Frost Attack's and Frost Breath's splash missile (CMissileFrostBreath).
    pub const FROST_SPLASH_MISSILE: &Code = b"Mfrb";
    /// `Rfrb`: Frost Attack's and Frost Breath's shell (CArtilleryFrostBreath).
    pub const FROST_ARTILLERY: &Code = b"Rfrb";
    /// `BTAR`: A bouncing missile's mark on each unit it is sent at (a buff, CMissileBounce
    /// `FUN_6f6d4700`; provisional name).
    pub const BOUNCE_TARGET: &Code = b"BTAR";
    /// `` `pbh ``: A missile's position (NIpse::CPoPosBh).
    pub const PROJECTILE_POSITION: &Code = b"`pbh";
    /// `>prj`: A missile's flight (NIpse::CBhPoProjectile).
    pub const PROJECTILE: &Code = b">prj";
}
