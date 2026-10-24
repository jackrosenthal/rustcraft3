//! What kind of buff each buff is, as game.dll's buff classes say (FUN_6f04daa0
//! registers each buff id under its class): magic (vf118) or physical (vf119), harmful
//! (vf120) or helpful (vf121), an aura's (vf122), a timed life (vf203).  Dispels and
//! the UnitRemoveBuffs, UnitHasBuffsEx, and UnitCountBuffsEx natives choose by these
//! (FUN_6f48eb10, FUN_6f48bd70).

use crate::types::Buff;
use crate::world::World;

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
    ("BAOE", POSITIVE), // CBuffAoe
    ("BAUR", AURA), // CBuffAura
    ("BBAR", MAGIC), // CBuffProgressBar
    ("BCbf", NEGATIVE), // CBuffBreathOfFrost
    ("BCtc", NEGATIVE), // CBuffCreepThunderClap
    ("BDoT", MAGIC), // CBuffDoT
    ("BEah", POSITIVE | AURA), // CBuffAuraThorns
    ("BEar", POSITIVE | AURA), // CBuffAuraTrueshot
    ("BEer", MAGIC | NEGATIVE), // CBuffEntanglingRoots
    ("BEfn", POSITIVE | TIMED_LIFE), // CBuffForceOfNature
    ("BEia", POSITIVE), // CBuffImmolationAoe
    ("BEim", RELATIVE), // CBuffImmolation
    ("BEme", POSITIVE), // CBuffMetamorphosis
    ("BEsh", NEGATIVE), // CBuffShadowStrike
    ("BEst", POSITIVE | TIMED_LIFE), // CBuffScout
    ("BEsv", POSITIVE | TIMED_LIFE), // CBuffSpiritOfVengeance
    ("BFig", POSITIVE | TIMED_LIFE), // CBuffFigurine
    ("BHab", POSITIVE | AURA), // CBuffAuraBrilliance
    ("BHad", POSITIVE | AURA), // CBuffAuraDevotion
    ("BHav", POSITIVE), // CBuffAvatar
    ("BHbd", NEGATIVE), // CBuffBlizzard
    ("BHbn", MAGIC | RELATIVE), // CBuffBanish
    ("BHbz", POSITIVE), // CBuffBlizzardAoe
    ("BHca", PHYSICAL | NEGATIVE), // CBuffColdArrow
    ("BHds", POSITIVE), // CBuffDivineShield
    ("BHfa", NEGATIVE), // CBuffFlameStrikeAoe
    ("BHfs", NEGATIVE), // CBuffFlameStrike
    ("BHtc", NEGATIVE), // CBuffThunderClap
    ("BHwe", POSITIVE | TIMED_LIFE), // CBuffWaterElemental
    ("BINV", MAGIC), // CBuff
    ("BIcb", NEGATIVE), // CBuffCorruption
    ("BIcf", RELATIVE), // CBuffItemCloakOfFlames
    ("BIil", POSITIVE | TIMED_LIFE), // CBuffItemIllusion
    ("BImo", POSITIVE | TIMED_LIFE), // CBuffItemMonsterLure
    ("BIpb", NEGATIVE), // CBuffPoisonDamage
    ("BIpd", NEGATIVE), // CBuffPoisonDamageStackDoT
    ("BIpi", NEGATIVE), // CBuffPoisonDamageStackInfo
    ("BIpv", MAGIC | POSITIVE), // CBuffItemVampirePotion
    ("BIrb", POSITIVE | TIMED_LIFE), // CBuffRebirth
    ("BIrg", POSITIVE), // CBuffRegeneration
    ("BIrl", POSITIVE), // CBuffRegenLife
    ("BIrm", POSITIVE), // CBuffRegenMana
    ("BIsh", POSITIVE | TIMED_LIFE), // CBuffSpiritTroll
    ("BIsv", NEGATIVE), // CBuffSoulTrapVision
    ("BIwb", MAGIC), // CBuffItemWeb
    ("BMDR", MAGIC), // CBuffManaDrain
    ("BNab", NEGATIVE), // CBuffAcidBomb
    ("BNba", NEGATIVE), // CBuffBlackArrow
    ("BNbf", NEGATIVE), // CBuffBreathOfFire
    ("BNbr", MAGIC | POSITIVE), // CBuffBattleRoar
    ("BNcg", POSITIVE | TIMED_LIFE), // CBuffClockworkGoblin
    ("BNcr", POSITIVE), // CBuffChemicalRage
    ("BNcs", POSITIVE), // CBuffClusterRockets
    ("BNdc", NEGATIVE), // CBuffDarkConversion
    ("BNdh", MAGIC | NEGATIVE), // CBuffDrunkenHaze
    ("BNdi", POSITIVE | TIMED_LIFE), // CBuffDoomMinion
    ("BNdm", POSITIVE | TIMED_LIFE), // CBuffDarkMinion
    ("BNdo", NEGATIVE), // CBuffDoom
    ("BNef", POSITIVE | TIMED_LIFE), // CBuffElementalFury
    ("BNfy", POSITIVE | TIMED_LIFE), // CBuffFactory
    ("BNhs", POSITIVE), // CBuffHealingSpray
    ("BNht", MAGIC | NEGATIVE), // CBuffHowlOfTerror
    ("BNic", NEGATIVE), // CBuffIncinerate
    ("BNin", POSITIVE | TIMED_LIFE), // CBuffInfernal
    ("BNlm", POSITIVE | TIMED_LIFE), // CBuffLavaMonster
    ("BNmr", MAGIC | NEGATIVE), // CBuffMindRot
    ("BNms", POSITIVE), // CBuffManaShield
    ("BNpa", PHYSICAL | NEGATIVE), // CBuffParasite
    ("BNpi", RELATIVE), // CBuffPermImmolation
    ("BNpm", POSITIVE | TIMED_LIFE), // CBuffParasiteMinion
    ("BNrd", NEGATIVE), // CBuffRainOfFire
    ("BNrf", POSITIVE), // CBuffRainOfFireAoe
    ("BNsa", MAGIC | POSITIVE), // CBuffSanctuary
    ("BNsg", POSITIVE | TIMED_LIFE), // CBuffSummonGrizzly
    ("BNsi", MAGIC | NEGATIVE), // CBuffSilence
    ("BNsl", NEGATIVE), // CBuffSoulPreservation
    ("BNso", MAGIC | NEGATIVE), // CBuffSoulBurn
    ("BNsq", POSITIVE | TIMED_LIFE), // CBuffSummonQuillbeast
    ("BNss", POSITIVE), // CBuffSpellShield
    ("BNst", MAGIC), // CBuffStampede
    ("BNsw", POSITIVE | TIMED_LIFE), // CBuffSummonWarEagle
    ("BNtm", NEGATIVE), // CBuffTransmute
    ("BNto", POSITIVE | TIMED_LIFE), // CBuffTornado
    ("BNva", POSITIVE), // CBuffVolcanoAoe
    ("BNwm", POSITIVE | TIMED_LIFE), // CBuffWateryMinion
    ("BOac", POSITIVE | AURA), // CBuffAuraCommand
    ("BOae", POSITIVE | AURA), // CBuffAuraEndurance
    ("BOea", POSITIVE), // CBuffEarthquakeAoe
    ("BOeq", NEGATIVE), // CBuffEarthquake
    ("BOhx", MAGIC | NEGATIVE), // CBuffHex
    ("BOmi", POSITIVE | TIMED_LIFE), // CBuffMirrorImage
    ("BOsf", POSITIVE | TIMED_LIFE), // CBuffSpiritWolf
    ("BOsh", NEGATIVE), // CBuffShockwave
    ("BOvc", POSITIVE), // CBuffVoodooCaster
    ("BOvd", POSITIVE | AURA), // CBuffVoodoo
    ("BOwd", POSITIVE | TIMED_LIFE), // CBuffWard
    ("BOwk", POSITIVE), // CBuffWindWalk
    ("BOww", POSITIVE), // CBuffWhirlwindAoe
    ("BPSE", NEGATIVE), // CBuffPause
    ("BSPD", MAGIC), // CBuffSpeed
    ("BSTI", NEGATIVE), // CBuffStackInfo
    ("BSTN", NEGATIVE), // CBuffStun
    ("BTAR", POSITIVE), // CBuffTarget
    ("BTLF", POSITIVE | TIMED_LIFE), // CBuffTimedLife
    ("BUan", POSITIVE | TIMED_LIFE), // CBuffAnimateDead
    ("BUau", POSITIVE | AURA), // CBuffAuraUnholy
    ("BUav", POSITIVE | AURA), // CBuffAuraVampiric
    ("BUcb", POSITIVE | TIMED_LIFE), // CBuffCarrionScarab
    ("BUcs", NEGATIVE), // CBuffCarrionSwarm
    ("BUdd", POSITIVE), // CBuffDeathAndDecayAoe
    ("BUfa", MAGIC | POSITIVE), // CBuffFrostArmor
    ("BUim", NEGATIVE), // CBuffImpale
    ("BUsl", MAGIC | NEGATIVE), // CBuffSleep
    ("BUsp", NEGATIVE), // CBuffSleepPause
    ("BUst", NEGATIVE), // CBuffSleepStun
    ("Babr", POSITIVE | AURA), // CBuffAuraBlightRegen
    ("Bakb", POSITIVE | AURA), // CBuffAuraKotoBeast
    ("Bam2", POSITIVE), // CBuffAntiMagicShellTwo
    ("Bams", MAGIC | POSITIVE), // CBuffAntiMagicShell
    ("Bapl", PHYSICAL | NEGATIVE), // CBuffAuraPlague
    ("Barm", POSITIVE | AURA), // CBuffAuraRegenMana
    ("Basl", NEGATIVE | AURA), // CBuffAuraSlow
    ("Bbar", MAGIC | POSITIVE), // CBuffBarkskin
    ("Bblo", MAGIC | POSITIVE), // CBuffBloodlust
    ("Bbof", NEGATIVE), // CBuffBallsOfFire
    ("Bbsk", POSITIVE), // CBuffBerserkerRage
    ("Bchd", NEGATIVE), // CBuffCargoHoldDeath
    ("Bclf", NEGATIVE), // CBuffCloudOfFog
    ("Bcmg", POSITIVE | TIMED_LIFE), // CBuffControlMagic
    ("Bcor", MAGIC | NEGATIVE), // CBuffCorrosiveBreath
    ("Bcri", MAGIC | NEGATIVE), // CBuffCripple
    ("Bcrs", MAGIC | NEGATIVE), // CBuffCurse
    ("Bcsd", PHYSICAL | NEGATIVE), // CBuffColdArrowStackDoT
    ("Bcsi", NEGATIVE), // CBuffColdArrowStackInfo
    ("Bcy2", MAGIC | NEGATIVE), // CBuffCycloneTwo
    ("Bcyc", NEGATIVE), // CBuffCyclone
    ("Bdbb", POSITIVE), // CBuffDrainBonus
    ("Bdbl", POSITIVE), // CBuffDrainBonusLife
    ("Bdbm", POSITIVE), // CBuffDrainBonusMana
    ("Bdcb", POSITIVE), // CBuffDrainCaster
    ("Bdcl", POSITIVE), // CBuffDrainCasterLife
    ("Bdcm", POSITIVE), // CBuffDrainCasterMana
    ("Bdef", MAGIC | POSITIVE), // CBuffDefense
    ("Bdet", NEGATIVE), // CBuffDetected
    ("Bdig", MAGIC | NEGATIVE), // CBuffDigesting
    ("Bdtb", POSITIVE), // CBuffDrainTarget
    ("Bdtl", POSITIVE), // CBuffDrainTargetLife
    ("Bdtm", POSITIVE), // CBuffDrainTargetMana
    ("Bdvv", NEGATIVE), // CBuffDevourVision
    ("Beat", MAGIC | POSITIVE), // CBuffEatTree
    ("Bena", MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffEnsnareAir
    ("Beng", MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffEnsnareGround
    ("Bens", MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffEnsnare
    ("Beth", MAGIC | RELATIVE), // CBuffEthereal
    ("Beye", POSITIVE | TIMED_LIFE), // CBuffEvilEye
    ("Bfae", MAGIC | NEGATIVE), // CBuffFaerieFire
    ("Bfre", NEGATIVE), // CBuffFreeze
    ("Bfro", NEGATIVE), // CBuffFrost
    ("Bfrz", NEGATIVE), // CBuffFreezingBreath
    ("Bfzy", MAGIC | POSITIVE), // CBuffFrenzy
    ("Bgra", POSITIVE), // CBuffGrabTree
    ("Bhea", POSITIVE), // CBuffHeal
    ("Bhwd", POSITIVE | TIMED_LIFE), // CBuffHealingWard
    ("Binf", MAGIC | POSITIVE), // CBuffInnerFire
    ("Binv", MAGIC | POSITIVE), // CBuffInvisibility
    ("BlDT", MAGIC), // CBuffLifeDoT
    ("Bliq", PHYSICAL | NEGATIVE), // CBuffLiquidFire
    ("Blsa", POSITIVE), // CBuffLightningShieldAoe
    ("Blsh", MAGIC | RELATIVE), // CBuffLightningShield
    ("Bmec", POSITIVE | TIMED_LIFE), // CBuffMechanicalCritter
    ("Bmfa", POSITIVE), // CBuffManaFlareAoe
    ("Bmfl", POSITIVE), // CBuffManaFlare
    ("Bmil", POSITIVE), // CBuffMilitia
    ("Bmlc", POSITIVE), // CBuffMagicLeashCaster
    ("Bmlt", NEGATIVE), // CBuffMagicLeashTarget
    ("Boar", POSITIVE | AURA), // CBuffAuraRegenLife
    ("Bphx", POSITIVE), // CBuffPhoenix
    ("Bpig", RELATIVE), // CBuffPandaImmolation
    ("Bplg", POSITIVE | TIMED_LIFE), // CBuffPlagueWard
    ("Bply", MAGIC | NEGATIVE), // CBuffPolymorph
    ("Bpoc", POSITIVE), // CBuffPossessionCaster
    ("Bpoi", PHYSICAL | NEGATIVE), // CBuffPoisonAttack
    ("Bpos", NEGATIVE), // CBuffPossession
    ("Bprg", NEGATIVE), // CBuffPurge
    ("Bprv", NEGATIVE), // CBuffPulseReveal
    ("Bpsd", PHYSICAL | NEGATIVE), // CBuffPoisonAttackStackDoT
    ("Bpsh", POSITIVE), // CBuffPhaseShift
    ("Bpsi", NEGATIVE), // CBuffPoisonAttackStackInfo
    ("Bpxf", PHYSICAL | NEGATIVE), // CBuffPhoenixFire
    ("Brai", POSITIVE | TIMED_LIFE), // CBuffRaiseDead
    ("Brej", MAGIC | POSITIVE), // CBuffRejuvination
    ("Brgl", MAGIC), // CBuffRangeListener
    ("Broa", MAGIC | POSITIVE), // CBuffRoar
    ("Brpb", POSITIVE), // CBuffReplenish
    ("Brpl", POSITIVE), // CBuffReplenishLife
    ("Brpm", POSITIVE), // CBuffReplenishMana
    ("Bsha", NEGATIVE), // CBuffSharedVision
    ("Bshs", MAGIC | NEGATIVE), // CBuffShadowSight
    ("Bslo", MAGIC | NEGATIVE), // CBuffSlow
    ("Bspa", NEGATIVE), // CBuffSpiderAttack
    ("Bspe", MAGIC | POSITIVE), // CBuffSpeedBonus
    ("Bspl", MAGIC | POSITIVE), // CBuffSpiritLink
    ("Bspo", PHYSICAL | NEGATIVE), // CBuffSlowPoison
    ("Bssd", PHYSICAL | NEGATIVE), // CBuffSlowPoisonStackDoT
    ("Bssi", NEGATIVE), // CBuffSlowPoisonStackInfo
    ("Bstt", POSITIVE | TIMED_LIFE), // CBuffStasisTrapTrigger
    ("Btau", NEGATIVE), // CBuffTaunt
    ("Btdg", POSITIVE), // CBuffTornadoDamageAoe
    ("Btsa", POSITIVE), // CBuffTornadoSpinAoe
    ("Btsp", MAGIC | NEGATIVE), // CBuffTornadoSpin
    ("Buhf", MAGIC | RELATIVE), // CBuffUnholyFrenzy
    ("Buns", POSITIVE), // CBuffUnsummon
    ("Bvng", POSITIVE | TIMED_LIFE), // CBuffVengeance
    ("Bvul", POSITIVE), // CBuffInvulnerable
    ("Bwea", MAGIC_ENSNARE | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffWebAir
    ("Bweb", MAGIC_WEB | PHYSICAL_UNLESS_MAGIC | NEGATIVE), // CBuffWeb
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
