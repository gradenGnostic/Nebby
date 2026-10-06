# Modern Camera

Enable under Mods for Pokémon Moon EUR v1.0. Settings are saved per title
and profile and applied on the next launch. Rebuild the native runtime once
to include the adapter; an older runtime is rejected when this mod is enabled.

Mouse motion controls yaw/pitch in relative mode. Controller right stick
orbits; R3 or the configured keyboard key recenters. Default keyboard movement
is WASD; C/V replace the conflicting original X/Y action bindings. Custom
bindings are preserved. Disabling the mod restores original bindings and
does not poll/capture camera input.

## Adapter contract

`core/modern_camera.h` has no guest addresses or renderer dependencies.
An adapter supplies an authoritative focus point in its game's units,
seconds elapsed, current pose and ownership. `mc_orbit_input` treats sticks
as angular velocity and mouse deltas as angular displacement. `mc_follow_step`
smooths position, focus and FOV; FOV is in radians. Seed `McFollow.pose` from
the original camera when acquiring ownership and reset on ownership loss.

`McCollisionQuery` returns nearest hit fraction (1), clear (0), or unknown
(-1). The adapter owns checked memory translation and the actual game query.
No host pointers or renderer matrices are exposed to this interface.

## Moon bindings

`adapters/moon/camera_probe.c` changes the game's BaseCamera look-at arguments
at `0x0040D2F8`, only for FieldRo area-controller caller `+0x33AFC`.
CameraManager, active unit/controller, player event/action state and camera
animation mode must identify free roam. Unknown/scripted ownership retains
the game's camera. FOV uses BaseCamera `+0xAC` (radians), not a renderer override.

Camera-relative movement uses FieldRo `+0x35DEC`, preserving the game's own
stick rotation. Collision calls retail FieldRo `+0x6FC94` against the player's
ground/wall scenes, using isolated registers and a checked, restored stack.
The current query is a thin ray, not a sphere sweep.

## Current limitations

Close-wall recovery checks neighboring azimuths using real world collision;
fully enclosed locations may still require a very close camera. Cutaway
interiors lack geometry from some orbit angles. Deeper battle/story ownership
testing remains the user's task; unrecognized states fail back to original.

Normal native builds use the packaged adapter automatically. Developer-only
`run-probe.sh` requires explicit workspace/library environment variables;
it refuses to launch a second Moon instance and keeps single-screen mode off.
