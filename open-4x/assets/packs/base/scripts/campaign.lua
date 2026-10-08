-- Sandboxed Lua 5.4, identical pure-Rust runtime on native and WASM.
-- No OS, filesystem, network, random numbers, or wall clock.
function on_turn(context)
    local industrial_boom = 100
    if context.turn % 12 == 0 then industrial_boom = 120 end
    return {
        income_percent = industrial_boom,
        fire_percent = 100,
        shock_percent = 100,
    }
end
