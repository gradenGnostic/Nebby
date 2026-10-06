#ifndef NEBBY_MODERN_CAMERA_H
#define NEBBY_MODERN_CAMERA_H

typedef struct { float x, y, z; } McVec3;
typedef struct { McVec3 position, target, up; float fov_y; } McPose;
typedef struct {
    float distance, height, pitch, yaw, smoothing_seconds, fov_y;
} McFollowConfig;
typedef struct { McPose pose; int initialized; } McFollow;

/* World axes: +Y up; yaw0 places the camera on target's +Z side.
 * Adapter supplies the game's authoritative focus point, in game units.
 * This core never accesses game memory or renderer objects. */
int mc_follow_step(McFollow *state, const McFollowConfig *config,
                   McVec3 focus, float seconds, McPose *output);
void mc_follow_reset(McFollow *state);
/* Adapter returns 1 with the nearest hit fraction, 0 for clear, -1 if unknown.
 * Unknown queries leave the pose untouched; the adapter owns fail-safe policy.
 * Radius is interpreted by the game's ray/sweep implementation. */
typedef int (*McCollisionQuery)(void *user, McVec3 start, McVec3 end,
                                float radius, float *hit_fraction);
int mc_resolve_collision(McPose *pose, float radius, float clearance,
                          McCollisionQuery query, void *user);
/* When the requested orbit collapses inside the character, search a bounded
 * set of neighboring azimuths. Every candidate uses the same world query.
 * Returns 2 for azimuth recovery; adopt the resolved yaw to avoid oscillation. */
int mc_resolve_collision_orbit(McPose *pose, float radius, float clearance,
                               float minimum_distance,
                               McCollisionQuery query, void *user);
/* Stick velocity is radians/second, mouse deltas radians/pixel. */
void mc_orbit_input(McFollowConfig *config, float stick_x, float stick_y,
                    float mouse_x, float mouse_y, float seconds,
                    float stick_speed, float mouse_speed, float deadzone,
                    int invert_y);
#endif
