#include "modern_camera.h"
#include <math.h>

void mc_follow_reset(McFollow *state) { state->initialized = 0; }
static float pose_distance(const McPose *pose) {
    return hypotf(hypotf(pose->position.x-pose->target.x,
                         pose->position.y-pose->target.y),
                  pose->position.z-pose->target.z);
}
int mc_resolve_collision_orbit(McPose *pose,float radius,float clearance,
                               float minimum_distance,
                               McCollisionQuery query,void *user) {
    if (!pose || !isfinite(minimum_distance) || minimum_distance<=0) return -1;
    McPose requested=*pose, best=requested;
    int result=mc_resolve_collision(&best,radius,clearance,query,user);
    if (result>=0 && pose_distance(&best)>=minimum_distance) {
        *pose=best;return result;
    }
    /* Unknown geometry is not permission to rotate around an obstruction. */
    if (result<0 && pose_distance(&requested)<=minimum_distance) return -1;
    const float angles[]={0.7853981634f,-0.7853981634f,
                           1.5707963268f,-1.5707963268f,3.1415926536f};
    float dx=requested.position.x-requested.target.x;
    float dz=requested.position.z-requested.target.z;
    for (unsigned i=0;i<sizeof(angles)/sizeof(angles[0]);++i) {
        float c=cosf(angles[i]),s=sinf(angles[i]);
        McPose candidate=requested;
        candidate.position.x=requested.target.x+c*dx+s*dz;
        candidate.position.z=requested.target.z-s*dx+c*dz;
        int hit=mc_resolve_collision(&candidate,radius,clearance,query,user);
        if (hit>=0 && pose_distance(&candidate)>=minimum_distance) {
            *pose=candidate;return 2;
        }
    }
    if (result>=0) {*pose=best;return result;}
    return -1;
}
int mc_resolve_collision(McPose *pose,float radius,float clearance,
                          McCollisionQuery query,void *user) {
    if (!pose || !query || !isfinite(radius) || radius<0 ||
        !isfinite(clearance) || clearance<0) return -1;
    McVec3 delta={pose->position.x-pose->target.x,
                  pose->position.y-pose->target.y,
                  pose->position.z-pose->target.z};
    float length=hypotf(hypotf(delta.x,delta.y),delta.z);
    if (!isfinite(length)) return -1;
    if (length<=0.00001f) return 0;
    float fraction=NAN;
    int hit=query(user,pose->target,pose->position,radius,&fraction);
    if (hit==0) return 0;
    if (hit!=1 || !isfinite(fraction) || fraction<0 || fraction>1) return -1;
    float scale=fmaxf(0,fraction-clearance/length);
    /* No valid look-at direction remains when the obstruction overlaps the
     * focus/clearance. Preserve the pose and let the adapter relinquish
     * ownership rather than submit an eye equal to its target. */
    if (scale*length<=0.00001f) return -1;
    pose->position=(McVec3){pose->target.x+delta.x*scale,
                            pose->target.y+delta.y*scale,
                            pose->target.z+delta.z*scale};
    return 1;
}
void mc_orbit_input(McFollowConfig *c, float x, float y,
                    float mx, float my, float seconds,
                    float stick_speed, float mouse_speed, float deadzone,
                    int invert_y) {
    if (!c || !isfinite(x) || !isfinite(y) || !isfinite(mx) || !isfinite(my) ||
        !isfinite(seconds) || seconds < 0 || seconds > 1 ||
        !isfinite(stick_speed) || !isfinite(mouse_speed) ||
        !isfinite(deadzone) || deadzone < 0 || deadzone >= 1) return;
    float magnitude = hypotf(x,y);
    if (magnitude <= deadzone) { x = 0; y = 0; }
    else {
        float scale = (fminf(magnitude,1)-deadzone)/((1-deadzone)*magnitude);
        x *= scale; y *= scale;
    }
    c->yaw = remainderf(c->yaw + x * stick_speed * seconds + mx * mouse_speed,
                        6.28318530718f);
    float delta = y * stick_speed * seconds - my * mouse_speed;
    c->pitch = fmaxf(-1.2f,fminf(1.3f,c->pitch + (invert_y ? -delta : delta)));
}
int mc_follow_step(McFollow *state, const McFollowConfig *c,
                   McVec3 focus, float seconds, McPose *output) {
    if (!state || !c || !output || !isfinite(seconds) || seconds < 0 ||
        !isfinite(c->distance) || c->distance <= 0 || !isfinite(c->height) ||
        !isfinite(c->pitch) || !isfinite(c->yaw) ||
        !isfinite(c->smoothing_seconds) || c->smoothing_seconds < 0 || !isfinite(c->fov_y) ||
        !isfinite(focus.x) || !isfinite(focus.y) || !isfinite(focus.z)) return 0;
    McPose desired = {0};
    desired.target = focus;
    desired.target.y += c->height;
    desired.up.y = 1;
    desired.fov_y=c->fov_y;
    float pitch = fmaxf(-1.4f, fminf(1.4f, c->pitch));
    float horizontal = c->distance * cosf(pitch);
    desired.position = (McVec3){desired.target.x + horizontal * sinf(c->yaw),
        desired.target.y + c->distance * sinf(pitch),
        desired.target.z + horizontal * cosf(c->yaw)};
    if (state->initialized && c->smoothing_seconds > 0) {
        float weight = -expm1f(-seconds / c->smoothing_seconds);
#define SMOOTH(field) desired.field = state->pose.field + weight * (desired.field - state->pose.field)
        SMOOTH(position.x); SMOOTH(position.y); SMOOTH(position.z);
        SMOOTH(target.x); SMOOTH(target.y); SMOOTH(target.z);
        SMOOTH(fov_y);
#undef SMOOTH
    }
    state->pose = desired;
    state->initialized = 1;
    *output = desired;
    return 1;
}
