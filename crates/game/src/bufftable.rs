//! What kind of buff each buff is, as game.dll's buff classes say (FUN_6f04daa0
//! registers each buff id under its class): magic (vf118) or physical (vf119), harmful
//! (vf120) or helpful (vf121), an aura's (vf122), a timed life (vf203).  Dispels and
//! the UnitRemoveBuffs, UnitHasBuffsEx, and UnitCountBuffsEx natives choose by these
//! (FUN_6f48eb10, FUN_6f48bd70).

use crate::types::Buff;
use crate::world::World;
use crate::codes;

pub const MAGIC: u16 = 1;
pub const PHYSICAL: u16 = 2;
pub const NEGATIVE: u16 = 4;
pub const POSITIVE: u16 = 8;
pub const AURA: u16 = 16;
pub const TIMED_LIFE: u16 = 32;
/// Harmful when an enemy of the bearer cast it, helpful otherwise (Banish, Ethereal,
/// Lightning Shield, Immolation, Unholy Frenzy).
const RELATIVE: u16 = 64;
/// Magic only under EnsnareIsMagic (Ensnare, and Web on air units).
const MAGIC_ENSNARE: u16 = 128;
/// Magic only under WebIsMagic.
const MAGIC_WEB: u16 = 256;
/// Physical when not magic.
const PHYSICAL_UNLESS_MAGIC: u16 = 512;

/// Buff ids and their classes' flags.
const BUFFS: &[(&str, u16)] = &[
    (codes::buff::AOE, POSITIVE), // CBuffAoe
    (codes::buff::AURA, AURA), // CBuffAura
    (codes::buff::PROGRESS_BAR, MAGIC), // CBuffProgressBar
    (codes::buff::BREATH_OF_FROST, NEGATIVE), // CBuffBreathOfFrost
    (codes::buff::SLAM, NEGATIVE), // CBuffCreepThunderClap
    (codes::buff::DOT, MAGIC), // CBuffDoT
    (codes::buff::THORNS_AURA, POSITIVE | AURA), // CBuffAuraThorns
    (codes::buff::TRUESHOT_AURA, POSITIVE | AURA), // CBuffAuraTrueshot
    (codes::buff::ENTANGLING_ROOTS, MAGIC | NEGATIVE), // CBuffEntanglingRoots
    (codes::buff::FORCE_OF_NATURE, POSITIVE | TIMED_LIFE), // CBuffForceOfNature
    (codes::buff::IMMOLATION_AOE, POSITIVE), // CBuffImmolationAoe
    (codes::buff::IMMOLATION, RELATIVE), // CBuffImmolation
    (codes::buff::METAMORPHOSIS, POSITIVE), // CBuffMetamorphosis
    (codes::buff::SHADOW_STRIKE, NEGATIVE), // CBuffShadowStrike
    (codes::buff::SCOUT, POSITIVE | TIMED_LIFE), // CBuffScout
    (codes::buff::VENGEANCE, POSITIVE | TIMED_LIFE), // CBuffSpiritOfVengeance
    (codes::buff::SUMMONED_UNIT, POSITIVE | TIMED_LIFE), // CBuffFigurine
    (codes::buff::BRILLIANCE_AURA, POSITIVE | AURA), // CBuffAuraBrilliance
    (codes::buff::DEVOTION_AURA, POSITIVE | AURA), // CBuffAuraDevotion
    (codes::buff::AVATAR, POSITIVE), // CBuffAvatar
    (codes::buff::BLIZZARD, NEGATIVE), // CBuffBlizzard
    (codes::buff::BANISH, MAGIC | RELATIVE), // CBuffBanish
    (codes::buff::BLIZZARD_AOE, POSITIVE), // CBuffBlizzardAoe
    (codes::buff::COLD_ARROWS_BHCA, PHYSICAL | NEGATIVE), // CBuffColdArrow
    (codes::buff::DIVINE_SHIELD, POSITIVE), // CBuffDivineShield
    (codes::buff::FLAME_STRIKE_AOE, NEGATIVE), // CBuffFlameStrikeAoe
    (codes::buff::FLAME_STRIKE, NEGATIVE), // CBuffFlameStrike
    (codes::buff::THUNDER_CLAP, NEGATIVE), // CBuffThunderClap
    (codes::buff::WATER_ELEMENTAL, POSITIVE | TIMED_LIFE), // CBuffWaterElemental
    (codes::buff::BINV, MAGIC), // CBuff
    (codes::buff::CORRUPTION, NEGATIVE), // CBuffCorruption
    (codes::buff::CLOAK_OF_FLAMES, RELATIVE), // CBuffItemCloakOfFlames
    (codes::buff::ILLUSION, POSITIVE | TIMED_LIFE), // CBuffItemIllusion
    (codes::buff::MONSTER_LURE, POSITIVE | TIMED_LIFE), // CBuffItemMonsterLure
    (codes::buff::VENOM_BIPB, NEGATIVE), // CBuffPoisonDamage
    (codes::buff::VENOM_BIPD, NEGATIVE), // CBuffPoisonDamageStackDoT
    (codes::buff::POISON_DAMAGE_STACK_INFO, NEGATIVE), // CBuffPoisonDamageStackInfo
    (codes::buff::VAMPIRIC_POTION, MAGIC | POSITIVE), // CBuffItemVampirePotion
    (codes::buff::REBORN, POSITIVE | TIMED_LIFE), // CBuffRebirth
    (codes::buff::REJUVENATION_BIRG, POSITIVE), // CBuffRegeneration
    (codes::buff::REGENERATION, POSITIVE), // CBuffRegenLife
    (codes::buff::CLARITY_POTION, POSITIVE), // CBuffRegenMana
    (codes::buff::HEADHUNTER_SPIRIT, POSITIVE | TIMED_LIFE), // CBuffSpiritTroll
    (codes::buff::SOUL_THEFT, NEGATIVE), // CBuffSoulTrapVision
    (codes::buff::ITEM_WEB, MAGIC), // CBuffItemWeb
    (codes::buff::MANA_DRAIN, MAGIC), // CBuffManaDrain
    (codes::buff::ACID_BOMB, NEGATIVE), // CBuffAcidBomb
    (codes::buff::BLACK_ARROW, NEGATIVE), // CBuffBlackArrow
    (codes::buff::BREATH_OF_FIRE, NEGATIVE), // CBuffBreathOfFire
    (codes::buff::BATTLE_ROAR, MAGIC | POSITIVE), // CBuffBattleRoar
    (codes::buff::CLOCKWERK_GOBLIN, POSITIVE | TIMED_LIFE), // CBuffClockworkGoblin
    (codes::buff::CHEMICAL_RAGE, POSITIVE), // CBuffChemicalRage
    (codes::buff::CLUSTER_ROCKETS, POSITIVE), // CBuffClusterRockets
    (codes::buff::DARK_CONVERSION, NEGATIVE), // CBuffDarkConversion
    (codes::buff::DRUNKEN_HAZE, MAGIC | NEGATIVE), // CBuffDrunkenHaze
    (codes::buff::DOOM_BNDI, POSITIVE | TIMED_LIFE), // CBuffDoomMinion
    (codes::buff::DARK_MINION, POSITIVE | TIMED_LIFE), // CBuffDarkMinion
    (codes::buff::DOOM_BNDO, NEGATIVE), // CBuffDoom
    (codes::buff::PANDAREN_ELEMENTAL, POSITIVE | TIMED_LIFE), // CBuffElementalFury
    (codes::buff::POCKET_FACTORY, POSITIVE | TIMED_LIFE), // CBuffFactory
    (codes::buff::HEALING_SPRAY, POSITIVE), // CBuffHealingSpray
    (codes::buff::HOWL_OF_TERROR, MAGIC | NEGATIVE), // CBuffHowlOfTerror
    (codes::buff::INCINERATE, NEGATIVE), // CBuffIncinerate
    (codes::buff::INFERNAL, POSITIVE | TIMED_LIFE), // CBuffInfernal
    (codes::buff::LAVA_SPAWN, POSITIVE | TIMED_LIFE), // CBuffLavaMonster
    (codes::buff::MIND_ROT, MAGIC | NEGATIVE), // CBuffMindRot
    (codes::buff::MANA_SHIELD, POSITIVE), // CBuffManaShield
    (codes::buff::PARASITE_BNPA, PHYSICAL | NEGATIVE), // CBuffParasite
    (codes::buff::PERM_IMMOLATION, RELATIVE), // CBuffPermImmolation
    (codes::buff::PARASITE_BNPM, POSITIVE | TIMED_LIFE), // CBuffParasiteMinion
    (codes::buff::RAIN_OF_FIRE, NEGATIVE), // CBuffRainOfFire
    (codes::buff::RAIN_OF_FIRE_AOE, POSITIVE), // CBuffRainOfFireAoe
    (codes::buff::SANCTUARY, MAGIC | POSITIVE), // CBuffSanctuary
    (codes::buff::BEAR, POSITIVE | TIMED_LIFE), // CBuffSummonGrizzly
    (codes::buff::SILENCE, MAGIC | NEGATIVE), // CBuffSilence
    (codes::buff::SOUL_PRESERVATION, NEGATIVE), // CBuffSoulPreservation
    (codes::buff::SOUL_BURN, MAGIC | NEGATIVE), // CBuffSoulBurn
    (codes::buff::QUILBEAST, POSITIVE | TIMED_LIFE), // CBuffSummonQuillbeast
    (codes::buff::SPELL_SHIELD, POSITIVE), // CBuffSpellShield
    (codes::buff::STAMPEDE, MAGIC), // CBuffStampede
    (codes::buff::HAWK, POSITIVE | TIMED_LIFE), // CBuffSummonWarEagle
    (codes::buff::TRANSMUTE, NEGATIVE), // CBuffTransmute
    (codes::buff::TORNADO_BNTO, POSITIVE | TIMED_LIFE), // CBuffTornado
    (codes::buff::VOLCANO_AOE, POSITIVE), // CBuffVolcanoAoe
    (codes::buff::WATERY_MINION, POSITIVE | TIMED_LIFE), // CBuffWateryMinion
    (codes::buff::AURA_COMMAND, POSITIVE | AURA), // CBuffAuraCommand
    (codes::buff::ENDURANCE_AURA, POSITIVE | AURA), // CBuffAuraEndurance
    (codes::buff::EARTHQUAKE_AOE, POSITIVE), // CBuffEarthquakeAoe
    (codes::buff::EARTHQUAKE, NEGATIVE), // CBuffEarthquake
    (codes::buff::HEX, MAGIC | NEGATIVE), // CBuffHex
    (codes::buff::MIRROR_IMAGE, POSITIVE | TIMED_LIFE), // CBuffMirrorImage
    (codes::buff::FERAL_SPIRIT, POSITIVE | TIMED_LIFE), // CBuffSpiritWolf
    (codes::buff::SHOCKWAVE, NEGATIVE), // CBuffShockwave
    (codes::buff::VOODOO_CASTER, POSITIVE), // CBuffVoodooCaster
    (codes::buff::BIG_BAD_VOODOO, POSITIVE | AURA), // CBuffVoodoo
    (codes::buff::WARD, POSITIVE | TIMED_LIFE), // CBuffWard
    (codes::buff::WIND_WALK, POSITIVE), // CBuffWindWalk
    (codes::buff::WHIRLWIND_AOE, POSITIVE), // CBuffWhirlwindAoe
    (codes::buff::STUNNED_BPSE, NEGATIVE), // CBuffPause
    (codes::buff::SPEED, MAGIC), // CBuffSpeed
    (codes::buff::STACK_INFO, NEGATIVE), // CBuffStackInfo
    (codes::buff::STUNNED_BSTN, NEGATIVE), // CBuffStun
    (codes::buff::TARGET, POSITIVE), // CBuffTarget
    (codes::buff::TIMED_LIFE, POSITIVE | TIMED_LIFE), // CBuffTimedLife
    (codes::buff::ANIMATE_DEAD, POSITIVE | TIMED_LIFE), // CBuffAnimateDead
    (codes::buff::UNHOLY_AURA, POSITIVE | AURA), // CBuffAuraUnholy
    (codes::buff::VAMPIRIC_AURA, POSITIVE | AURA), // CBuffAuraVampiric
    (codes::buff::CARRION_BEETLES, POSITIVE | TIMED_LIFE), // CBuffCarrionScarab
    (codes::buff::CARRION_SWARM, NEGATIVE), // CBuffCarrionSwarm
    (codes::buff::DEATH_AND_DECAY, POSITIVE), // CBuffDeathAndDecayAoe
    (codes::buff::FROST_ARMOR, MAGIC | POSITIVE), // CBuffFrostArmor
    (codes::buff::IMPALE, NEGATIVE), // CBuffImpale
    (codes::buff::SLEEP, MAGIC | NEGATIVE), // CBuffSleep
    (codes::buff::SLEEP_PAUSE, NEGATIVE), // CBuffSleepPause
    (codes::buff::SLEEP_STUN, NEGATIVE), // CBuffSleepStun
    (codes::buff::AURA_OF_BLIGHT, POSITIVE | AURA), // CBuffAuraBlightRegen
    (codes::buff::WAR_DRUMS, POSITIVE | AURA), // CBuffAuraKotoBeast
    (codes::buff::ANTI_MAGIC_SHELL_BAM2, POSITIVE), // CBuffAntiMagicShellTwo
    (codes::buff::ANTI_MAGIC_SHELL_BAMS, MAGIC | POSITIVE), // CBuffAntiMagicShell
    (codes::buff::DISEASE, PHYSICAL | NEGATIVE), // CBuffAuraPlague
    (codes::buff::MANA_REGENERATION_AURA, POSITIVE | AURA), // CBuffAuraRegenMana
    (codes::buff::TORNADO_BASL, NEGATIVE | AURA), // CBuffAuraSlow
    (codes::buff::BARKSKIN, MAGIC | POSITIVE), // CBuffBarkskin
    (codes::buff::BLOODLUST, MAGIC | POSITIVE), // CBuffBloodlust
    (codes::buff::BURNING_OIL, NEGATIVE), // CBuffBallsOfFire
    (codes::buff::BERSERK, POSITIVE), // CBuffBerserkerRage
    (codes::buff::DIZZINESS, NEGATIVE), // CBuffCargoHoldDeath
    (codes::buff::CLOUD, NEGATIVE), // CBuffCloudOfFog
    (codes::buff::CONTROL_MAGIC, POSITIVE | TIMED_LIFE), // CBuffControlMagic
    (codes::buff::CORROSIVE_BREATH, MAGIC | NEGATIVE), // CBuffCorrosiveBreath
    (codes::buff::CRIPPLE, MAGIC | NEGATIVE), // CBuffCripple
    (codes::buff::CURSE, MAGIC | NEGATIVE), // CBuffCurse
    (codes::buff::COLD_ARROWS_BCSD, PHYSICAL | NEGATIVE), // CBuffColdArrowStackDoT
    (codes::buff::COLD_ARROW_STACK_INFO, NEGATIVE), // CBuffColdArrowStackInfo
    (codes::buff::CYCLONE_BCY2, MAGIC | NEGATIVE), // CBuffCycloneTwo
    (codes::buff::CYCLONE_BCYC, NEGATIVE), // CBuffCyclone
    (codes::buff::DRAIN_LIFE_AND_MANA, POSITIVE), // CBuffDrainBonus
    (codes::buff::DRAIN_LIFE, POSITIVE), // CBuffDrainBonusLife
    (codes::buff::SIPHON_MANA, POSITIVE), // CBuffDrainBonusMana
    (codes::buff::DRAIN_CASTER, POSITIVE), // CBuffDrainCaster
    (codes::buff::DRAIN_CASTER_LIFE, POSITIVE), // CBuffDrainCasterLife
    (codes::buff::DRAIN_CASTER_MANA, POSITIVE), // CBuffDrainCasterMana
    (codes::buff::SCROLL_OF_PROTECTION, MAGIC | POSITIVE), // CBuffDefense
    (codes::buff::DUST_OF_APPEARANCE, NEGATIVE), // CBuffDetected
    (codes::buff::DIGESTING, MAGIC | NEGATIVE), // CBuffDigesting
    (codes::buff::DRAIN_TARGET, POSITIVE), // CBuffDrainTarget
    (codes::buff::DRAIN_TARGET_LIFE, POSITIVE), // CBuffDrainTargetLife
    (codes::buff::DRAIN_TARGET_MANA, POSITIVE), // CBuffDrainTargetMana
    (codes::buff::DEVOUR, NEGATIVE), // CBuffDevourVision
    (codes::buff::EAT_TREE, MAGIC | POSITIVE), // CBuffEatTree
    (codes::buff::ENSNARE_BENA, MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffEnsnareAir
    (codes::buff::ENSNARE_BENG, MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffEnsnareGround
    (codes::buff::ENSNARE_BENS, MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffEnsnare
    (codes::buff::ETHEREAL, MAGIC | RELATIVE), // CBuffEthereal
    (codes::buff::SENTRY_WARD, POSITIVE | TIMED_LIFE), // CBuffEvilEye
    (codes::buff::FAERIE_FIRE, MAGIC | NEGATIVE), // CBuffFaerieFire
    (codes::buff::FREEZE, NEGATIVE), // CBuffFreeze
    (codes::buff::SLOWED, NEGATIVE), // CBuffFrost
    (codes::buff::FREEZING_BREATH, NEGATIVE), // CBuffFreezingBreath
    (codes::buff::FRENZY, MAGIC | POSITIVE), // CBuffFrenzy
    (codes::buff::GRAB_TREE, POSITIVE), // CBuffGrabTree
    (codes::buff::HEAL, POSITIVE), // CBuffHeal
    (codes::buff::HEALING_WARD, POSITIVE | TIMED_LIFE), // CBuffHealingWard
    (codes::buff::INNER_FIRE, MAGIC | POSITIVE), // CBuffInnerFire
    (codes::buff::INVISIBILITY, MAGIC | POSITIVE), // CBuffInvisibility
    (codes::buff::LIFE_DOT, MAGIC), // CBuffLifeDoT
    (codes::buff::LIQUID_FIRE, PHYSICAL | NEGATIVE), // CBuffLiquidFire
    (codes::buff::LIGHTNING_SHIELD_AOE, POSITIVE), // CBuffLightningShieldAoe
    (codes::buff::LIGHTNING_SHIELD, MAGIC | RELATIVE), // CBuffLightningShield
    (codes::buff::MECHANICAL_CRITTER, POSITIVE | TIMED_LIFE), // CBuffMechanicalCritter
    (codes::buff::MANA_FLARE_BMFA, POSITIVE), // CBuffManaFlareAoe
    (codes::buff::MANA_FLARE_BMFL, POSITIVE), // CBuffManaFlare
    (codes::buff::MILITIA, POSITIVE), // CBuffMilitia
    (codes::buff::MAGIC_LEASH_CASTER, POSITIVE), // CBuffMagicLeashCaster
    (codes::buff::AERIAL_SHACKLES, NEGATIVE), // CBuffMagicLeashTarget
    (codes::buff::HEALING_WARD_AURA, POSITIVE | AURA), // CBuffAuraRegenLife
    (codes::buff::PHOENIX, POSITIVE), // CBuffPhoenix
    (codes::buff::PANDA_IMMOLATION, RELATIVE), // CBuffPandaImmolation
    (codes::buff::PLAGUE_WARD, POSITIVE | TIMED_LIFE), // CBuffPlagueWard
    (codes::buff::POLYMORPH, MAGIC | NEGATIVE), // CBuffPolymorph
    (codes::buff::POSSESSION_BPOC, POSITIVE), // CBuffPossessionCaster
    (codes::buff::POISON_BPOI, PHYSICAL | NEGATIVE), // CBuffPoisonAttack
    (codes::buff::POSSESSION_BPOS, NEGATIVE), // CBuffPossession
    (codes::buff::PURGE, NEGATIVE), // CBuffPurge
    (codes::buff::PULSE_REVEAL, NEGATIVE), // CBuffPulseReveal
    (codes::buff::POISON_BPSD, PHYSICAL | NEGATIVE), // CBuffPoisonAttackStackDoT
    (codes::buff::PHASE_SHIFT, POSITIVE), // CBuffPhaseShift
    (codes::buff::POISON_ATTACK_STACK_INFO, NEGATIVE), // CBuffPoisonAttackStackInfo
    (codes::buff::PHOENIX_FIRE, PHYSICAL | NEGATIVE), // CBuffPhoenixFire
    (codes::buff::SKELETAL_MINION, POSITIVE | TIMED_LIFE), // CBuffRaiseDead
    (codes::buff::REJUVENATION_BREJ, MAGIC | POSITIVE), // CBuffRejuvination
    (codes::buff::RANGE_LISTENER, MAGIC), // CBuffRangeListener
    (codes::buff::ROAR, MAGIC | POSITIVE), // CBuffRoar
    (codes::buff::REPLENISH, POSITIVE), // CBuffReplenish
    (codes::buff::ESSENCE_OF_BLIGHT, POSITIVE), // CBuffReplenishLife
    (codes::buff::SPIRIT_TOUCH, POSITIVE), // CBuffReplenishMana
    (codes::buff::SHARED_VISION, NEGATIVE), // CBuffSharedVision
    (codes::buff::WAND_OF_SHADOWSIGHT, MAGIC | NEGATIVE), // CBuffShadowSight
    (codes::buff::SLOW, MAGIC | NEGATIVE), // CBuffSlow
    (codes::buff::SPIDERLING, NEGATIVE), // CBuffSpiderAttack
    (codes::buff::SPEED_BONUS, MAGIC | POSITIVE), // CBuffSpeedBonus
    (codes::buff::SPIRIT_LINK, MAGIC | POSITIVE), // CBuffSpiritLink
    (codes::buff::SLOW_POISON_BSPO, PHYSICAL | NEGATIVE), // CBuffSlowPoison
    (codes::buff::SLOW_POISON_BSSD, PHYSICAL | NEGATIVE), // CBuffSlowPoisonStackDoT
    (codes::buff::SLOW_POISON_STACK_INFO, NEGATIVE), // CBuffSlowPoisonStackInfo
    (codes::buff::STASIS_TRAP, POSITIVE | TIMED_LIFE), // CBuffStasisTrapTrigger
    (codes::buff::TAUNT, NEGATIVE), // CBuffTaunt
    (codes::buff::TORNADO_DAMAGE_AOE, POSITIVE), // CBuffTornadoDamageAoe
    (codes::buff::TORNADO_SPIN_AOE, POSITIVE), // CBuffTornadoSpinAoe
    (codes::buff::TORNADO_SPIN, MAGIC | NEGATIVE), // CBuffTornadoSpin
    (codes::buff::UNHOLY_FRENZY, MAGIC | RELATIVE), // CBuffUnholyFrenzy
    (codes::buff::UNSUMMON, POSITIVE), // CBuffUnsummon
    (codes::buff::SPIRIT_OF_VENGEANCE, POSITIVE | TIMED_LIFE), // CBuffVengeance
    (codes::buff::INVULNERABLE, POSITIVE), // CBuffInvulnerable
    (codes::buff::WEB_BWEA, MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffWebAir
    (codes::buff::WEB_BWEB, MAGIC_WEB | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffWeb
];

/// What a dispel or a buff native may choose by (FUN_6f48eb10's arguments).
#[derive(Clone, Copy, Debug, Default)]
pub struct Filter {
    pub positive: bool,
    pub negative: bool,
    /// Only magic buffs.
    pub magic: bool,
    /// Only physical buffs.
    pub physical: bool,
    /// Timed lives too.
    pub timed_life: bool,
    /// Auras' buffs too.
    pub aura: bool,
    /// Only buffs an automatic dispel takes (the magic ones).
    pub auto_dispel: bool,
}

impl World {
    /// A buff's flags, by its id or else its ability's own buff, harmful or helpful
    /// settled for its bearer; a buff the table does not know is the plain CBuff
    /// (magic, neither side).
    pub fn buff_flags(&self, b: &Buff, bearer: u8) -> u16 {
        let code = crate::objects::id_str(b.id);
        let row = BUFFS.iter().find(|r| r.0 == code).or_else(|| {
            let base = crate::objects::id_from_str(&self.ability_code(b.ability));
            let ids = self.objects.abilities.get(base).map(|o| o.level_str("BuffID", 1)).unwrap_or_default();
            ids.split(',').find_map(|id| BUFFS.iter().find(|r| r.0 == id.trim()))
        });
        let mut f = row.map_or(MAGIC, |r| r.1);
        if f & MAGIC_ENSNARE != 0 && self.misc_f32("EnsnareIsMagic", 0.0) != 0.0 || f & MAGIC_WEB != 0 && self.misc_f32("WebIsMagic", 0.0) != 0.0 {
            f |= MAGIC;
        }
        if f & PHYSICAL_UNLESS_MAGIC != 0 && f & MAGIC == 0 {
            f |= PHYSICAL;
        }
        if f & RELATIVE != 0 {
            let enemy = !self.unit(b.caster).is_some_and(|c| crate::natives::players::is_ally(self, bearer, c.owner));
            f |= if enemy { NEGATIVE } else { POSITIVE };
        }
        f & (MAGIC | PHYSICAL | NEGATIVE | POSITIVE | AURA | TIMED_LIFE)
    }

    /// Whether a removal takes a buff (FUN_6f48eb10): helpful ones under `positive`,
    /// harmful ones under `negative`, then the other conditions.
    pub fn buff_removable(&self, b: &Buff, bearer: u8, f: Filter) -> bool {
        let flags = self.buff_flags(b, bearer);
        ((f.positive && flags & POSITIVE != 0) || (f.negative && flags & NEGATIVE != 0)) && Self::buff_passes(flags, f)
    }

    /// Whether a count takes a buff (FUN_6f48bd70): any helpful or harmful one, except
    /// a helpful one when only harmful ones are asked for and the other way about.
    pub fn buff_counted(&self, b: &Buff, bearer: u8, f: Filter) -> bool {
        let flags = self.buff_flags(b, bearer);
        let (pos, neg) = (flags & POSITIVE != 0, flags & NEGATIVE != 0);
        (pos || neg) && !(pos && !f.positive && f.negative) && !(neg && !f.negative && f.positive) && Self::buff_passes(flags, f)
    }

    fn buff_passes(flags: u16, f: Filter) -> bool {
        (f.aura || flags & AURA == 0)
            && (f.timed_life || flags & TIMED_LIFE == 0)
            && (!f.physical || flags & PHYSICAL != 0)
            && (!f.magic || flags & MAGIC != 0)
            && (!f.auto_dispel || flags & MAGIC != 0)
    }
}
