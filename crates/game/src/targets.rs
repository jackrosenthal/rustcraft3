//! Spell targets: an ability level's allowed targets (AbilityData targs) against what a
//! target is (its targType classes and its type classifications), with the command
//! error for a target that is not allowed.

use crate::objects::Id;

/// ALLIANCE_SHARED_SPELLS.
const ALLIANCE_SHARED_SPELLS: usize = 4;
use crate::types::*;
use crate::world::World;

/// Target classes, and the errors for a target of a class that is not allowed: when
/// only one class is allowed, and when several are.
const CLASSES: [(&str, &str, &str); 9] = [
    ("air", "Targetair", "Noair"),
    ("ground", "Targetground", "Noground"),
    ("structure", "Targetstructure", "Nostructure"),
    ("ward", "Targetward", "Noward"),
    ("tree", "Targettree", "Notree"),
    ("debris", "Targetunit", "Nodebris"),
    ("wall", "Targetunit", "Nowall"),
    ("bridge", "Targetbridge", "Nobridge"),
    ("item", "Targetitem", "Noitem"),
];

impl World {
    /// Why `target` is not a valid target of ability `a` cast by `h`, as a command
    /// error key; None when it is.
    pub fn cast_target_error(&self, h: u32, a: Id, target: u32) -> Option<&'static str> {
        let lvl = self.ability_level(h, a).max(1);
        let flags: Vec<String> = self
            .objects
            .abilities
            .get(a)
            .map(|o| o.level_str("targs", lvl))
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty() && s != "_" && s != "none")
            .collect();
        if flags.is_empty() {
            return None;
        }
        let has = |f: &str| flags.iter().any(|x| x == f);
        let caster = self.unit(h)?;
        // What the target is.
        let (classes, unit): (Vec<String>, Option<&Unit>) = match self.handles.get(target) {
            Some(Obj::Unit(u)) => {
                let t = self.objects.units.get(u.type_id).map(|o| o.str("targType")).unwrap_or_default();
                (t.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect(), Some(u))
            }
            Some(Obj::Destructable(d)) => {
                let t = self.objects.destructables.get(d.type_id).map(|o| o.str("targType")).unwrap_or_default();
                (t.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect(), None)
            }
            Some(Obj::Item(_)) => (vec!["item".to_string()], None),
            _ => return Some("Targetunit"),
        };
        let allowed: Vec<&(&str, &str, &str)> = CLASSES.iter().filter(|c| has(c.0)).collect();
        if !allowed.is_empty() && !classes.iter().any(|c| allowed.iter().any(|a| a.0 == c)) {
            if allowed.len() == 1 {
                return Some(allowed[0].1);
            }
            let class = classes.first().map(|s| s.as_str()).unwrap_or("ground");
            return Some(CLASSES.iter().find(|c| c.0 == class).map_or("Notthisunit", |c| c.2));
        }
        let Some(t) = unit else { return None };
        if self.is_locust(t) {
            return Some("Notthisunit");
        }
        if has("notself") && t.handle == h {
            return Some("Notself");
        }
        // Who the target belongs to.
        let relations = ["self", "player", "ally", "friend", "neutral", "enemy"];
        if relations.iter().any(|r| has(r)) {
            // As the original classifies: the target's owner not passive toward the
            // caster's is an enemy; passive and sharing spells, an ally; passive
            // alone, neutral.
            let own = t.owner == caster.owner;
            let toward = |k: usize| self.players.get(t.owner as usize).is_some_and(|p| p.alliances[caster.owner as usize % PLAYER_COUNT][k]);
            let passive = toward(crate::natives::players::ALLIANCE_PASSIVE);
            let allied = !own && passive && toward(ALLIANCE_SHARED_SPELLS);
            let neutral = !own && passive && !allied;
            let ok = if t.handle == h {
                has("self")
            } else if own {
                has("player") || has("friend")
            } else if neutral {
                has("neutral")
            } else if allied {
                has("ally") || has("friend")
            } else {
                has("enemy")
            };
            if !ok {
                let only_enemy = has("enemy") && !has("player") && !has("ally") && !has("friend") && !has("self");
                let only_friend = (has("friend") || has("ally") || has("player")) && !has("enemy");
                return Some(if t.handle == h {
                    "Notself"
                } else if only_enemy {
                    "Targetenemy"
                } else if neutral {
                    "Notneutral"
                } else if own && has("ally") && !has("player") && !has("friend") {
                    "Notowned"
                } else if own || allied {
                    "Notfriendly"
                } else if has("player") && !has("ally") && !has("friend") {
                    "Targetowned"
                } else if only_friend {
                    "Targetally"
                } else {
                    "Notenemy"
                });
            }
        }
        // Invulnerable targets only when allowed; vulnerable ones unless only
        // invulnerable ones are.
        let invulnerable = self.unit_invulnerable(t);
        if invulnerable && !has("invu") {
            return Some("Notinvulnerable");
        }
        if !invulnerable && has("invu") && !has("vuln") {
            return Some("Notthisunit");
        }
        if t.dead && !has("dead") {
            return Some("Notcorpse");
        }
        if !t.dead && has("dead") && !has("alive") {
            return Some("Targetcorpse");
        }
        if t.is_hero && has("nonhero") && !has("hero") {
            return Some("Nohero");
        }
        if !t.is_hero && has("hero") && !has("nonhero") {
            return Some("Targethero");
        }
        let kinds = self.objects.units.get(t.type_id).map(|o| o.str("type").to_ascii_lowercase()).unwrap_or_default();
        let kind = |k: &str| kinds.split(',').any(|x| x.trim() == k);
        let mechanical = kind("mechanical");
        if mechanical && has("organic") && !has("mechanical") {
            return Some("Notmechanical");
        }
        if !mechanical && has("mechanical") && !has("organic") {
            return Some("Targetmechanical");
        }
        if kind("ancient") && has("nonancient") && !has("ancient") {
            return Some("Notancient");
        }
        if !kind("ancient") && has("ancient") && !has("nonancient") {
            return Some("Targetancient");
        }
        if kind("sapper") && has("nonsapper") && !has("sapper") {
            return Some("Notsapper");
        }
        None
    }
}
