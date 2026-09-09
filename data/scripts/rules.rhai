// Islefall rule hooks. The simulation calls these at decision points; the
// numbers they work with come from rules.toml. Scripts must be
// deterministic: same inputs, same result, no randomness, no time.

// Energy a type needs at its placement site, from its `level`, `theme` and
// `mana` string. Returns #{ theme: "sun"|"wind"|"rain"|"thunder", themed: n, any: n }
// or () when the type needs none.
//
// The manual's examples: a Level One Bulf needs one Thunder, a Level Two
// Whirlibase two Sun, a Level Three Thunder Cannon two Thunder and one Sun,
// an Air Ship (Level Three, Wind) two Wind and one Sun.
fn energy_need(level, theme, mana, has_class) {
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
// their `threat`, units theirs; ties go to the nearest target.
fn target_priority(threat, distance, is_unit) {
    threat * 1000 - distance
}
