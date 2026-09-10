// Islefall rule hooks. The simulation calls these at decision points; the
// numbers they work with come from rules.toml. Scripts must be
// deterministic: same inputs, same result, no randomness, no time.

// Energy a type needs at its placement site, from its `level`, `theme`,
// `mana` string and `class` ("" when the type has none). Returns #{ theme: "sun"|"wind"|"rain"|"thunder", themed: n, any: n }
// or () when the type needs none.
//
// The manual's examples: a Level One Bulf needs one Thunder, a Level Two
// Whirlibase two Sun, a Level Three Thunder Cannon two Thunder and one Sun,
// an Air Ship (Level Three, Wind) two Wind and one Sun.
fn energy_need(level, theme, mana, class) {
    let has_class = class != "";
    // A generator brings its own Fury's Energy, so it asks for any Energy
    // rather than its own theme's: the first Wind Generator stands on the
    // Temple's Sun, and the next on the first (the tutorial's Sun Workshop
    // producing a Wind Generator).
    if class == "Source of Energy" && level > 0 {
        return #{ theme: "sun", themed: 0, any: level };
    }
    if mana != "" {
        let need = #{ theme: "sun", themed: 0, any: 0 };
        for ch in mana.to_lower().chars() {
            switch ch {
                's' => need.any += 1,
                'w' => { need.theme = "wind"; need.themed += 1; }
                'r' => { need.theme = "rain"; need.themed += 1; }
                't' => { need.theme = "thunder"; need.themed += 1; }
                _ => {}
            }
        }
        return if need.themed + need.any > 0 { need } else { () };
    }
    if level <= 0 || !has_class {
        return ();
    }
    if theme == "sun" {
        return #{ theme: "sun", themed: 0, any: level };
    }
    if level == 1 {
        return #{ theme: theme, themed: 1, any: 0 };
    }
    #{ theme: theme, themed: level - 1, any: 1 }
}

// Whether a type fires only straight north, south, east or west. The manual
// says so of cannons; no flag carries it, so match the name.
fn fires_straight(name, flags) {
    name.to_lower().contains("cannon")
}

// Damage of one shot from `hpPerSec` and the delay between shots in seconds.
fn damage_per_shot(hp_per_sec, delay) {
    (hp_per_sec * delay).round().to_int()
}

// Storm Power refunded for salvaging a structure: a quarter of the cost,
// scaled by remaining health (manual: less for damaged units).
fn salvage_refund(cost, hp, max_hp, refund_percent) {
    let health = if max_hp > 0 { hp * 100 / max_hp } else { 100 };
    cost * refund_percent * health / 10000
}

// Which Knowledge bit a sacrifice grants: the lowest one the owner lacks.
// `known` and `all` are arrays of bits.
fn knowledge_grant(known, all) {
    for bit in all {
        if !known.contains(bit) {
            return bit;
        }
    }
    ()
}

// Targeting priority for a shooter: higher is preferred. Structures use
// their `threat`, units theirs; ties go to the nearest target. A shooter
// stays on the target it is already firing at (`is_current`) while that
// remains in range, rather than swinging to every newcomer.
fn target_priority(threat, distance, is_unit, is_current) {
    threat * 1000 - distance + if is_current { 1000000 } else { 0 }
}

// Whether a Workshop of `workshop_theme` may put a type of `type_theme` and
// `type_level` into production. The tutorial has a Sun Workshop producing
// a Wind Generator, so alignment is only demanded of higher-level units.
fn workshop_can_produce(workshop_theme, type_theme, type_level) {
    type_theme == "sun" || workshop_theme == type_theme || type_level <= 1
}

// What a structure can shoot at, from its class and the `useairdamage`,
// `airrange`, `airdamage`, `range` and `hpPerSec` properties. Returns
// #{ air_range, air_damage, ground } or () for a type that never fires.
// The Vander Tower ("Anti-Air") shoots only at flyers; a Shooter with air
// damage shoots both; anything else only at the ground.
fn air_attack(class, use_air_damage, air_range, air_damage, range, hp_per_sec) {
    if class == "Anti-Air" {
        return #{ air_range: air_range, air_damage: air_damage, ground: false };
    }
    if hp_per_sec <= 0 {
        return ();
    }
    if air_damage > 0 && air_range > 0 {
        return #{ air_range: air_range, air_damage: air_damage, ground: true };
    }
    #{ air_range: 0, air_damage: 0, ground: true }
}

// Targeting priority of an aerial attacker: higher wins, () means never.
// The manual: attackers take the closest target; a Whirligig never targets
// a Transport; a Man o'War prefers Ground Transports.
fn air_target_priority(distance, is_unit, is_transport, hunts_transports) {
    if is_transport && !hunts_transports {
        return ();
    }
    let bonus = if is_transport { 1000 } else { 0 };
    bonus - distance
}

// Gain in decibels of a sound `edge` of the way from the camera centre to
// the edge of the view (0 at the centre, 1 at the edge), at camera `zoom`.
// `base_db` is the file's own gain. The camera's height costs
// `db_per_halving` for each halving of the zoom below `reference_zoom`;
// zooming in closer than that adds nothing.
fn sound_gain(edge, zoom, base_db, edge_db, reference_zoom, db_per_halving) {
    let halvings = if zoom > 0.0 && reference_zoom > zoom { (reference_zoom / zoom).ln() / 2.0.ln() } else { 0.0 };
    base_db + edge * edge_db - halvings * db_per_halving
}

// Seconds a stream takes to build a placed structure, from its cost, its
// `constructionRate` and the rules' Power per second per rate unit.
// Types without a rate (the manual's "in a moment") take a second.
fn construction_seconds(cost, construction_rate, power_per_rate) {
    if construction_rate <= 0.0 || power_per_rate <= 0.0 || cost <= 0 {
        return 1.0;
    }
    cost / (construction_rate * power_per_rate)
}

// Storm Power awarded for destroying an enemy's unit or structure: a share
// of its cost (the manual: 25%, adjustable by the Battlemaster).
fn kill_reward(cost, percent) {
    cost * percent / 100
}

// What a shot does to a bridge cell in `state` ("normal", "cracked" or
// "hard"): "crack", "destroy" or "none". The manual: explosions crack
// sound bridges and destroy cracked ones; shots are read the same way,
// and hardened bridges are indestructible.
fn bridge_hit(state, damage) {
    switch state {
        "hard" => "none",
        "cracked" => "destroy",
        _ => "crack",
    }
}
