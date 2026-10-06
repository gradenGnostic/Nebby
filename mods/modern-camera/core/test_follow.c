#include "modern_camera.h"
#include <assert.h>
#include <math.h>
static void close_to(float a, float b) { assert(fabsf(a-b)<0.002f); }
static int obstruction(void *user,McVec3 start,McVec3 end,float radius,float *fraction) {
    (void)start;(void)end;assert(radius==10);
    *fraction=*(float *)user;return 1;
}
static int wall(void *user,McVec3 start,McVec3 end,float radius,float *fraction) {
    (void)start;(void)radius;
    ++*(int *)user;
    if (end.z>50) {*fraction=50/end.z;return 1;}
    return 0;
}
int main(void) {
    McFollow state = {0}; McPose pose;
    McFollowConfig config = {600, 0, 0, 0, 0.1f, 0.8f};
    McVec3 focus = {10, 20, 30};
    assert(mc_follow_step(&state, &config, focus, 1.f/30, &pose));
    close_to(pose.position.x,10); close_to(pose.position.y,20); close_to(pose.position.z,630);
    config.yaw = 1.57079632679f; mc_follow_reset(&state);
    assert(mc_follow_step(&state,&config,focus,1.f/30,&pose));
    close_to(pose.position.x,610); close_to(pose.position.z,30);
    McFollow once = state, twice = state; McPose a,b; focus.x += 100;
    assert(mc_follow_step(&once,&config,focus,1.f/30,&a));
    assert(mc_follow_step(&twice,&config,focus,1.f/60,&b));
    assert(mc_follow_step(&twice,&config,focus,1.f/60,&b));
    close_to(a.position.x,b.position.x);
    close_to(a.fov_y,b.fov_y);
    config.distance = NAN; assert(!mc_follow_step(&state,&config,focus,1.f/30,&pose));
    config.yaw=0; config.pitch=0;
    mc_orbit_input(&config,0.1f,0,0,0,1.f/30,2,0.01f,0.15f,0);
    close_to(config.yaw,0);
    mc_orbit_input(&config,1,0,0,0,0.5f,2,0.01f,0.15f,0);
    close_to(config.yaw,1);
    mc_orbit_input(&config,0,0,10,10,0,2,0.01f,0.15f,0);
    close_to(config.yaw,1.1f); close_to(config.pitch,-0.1f);
    mc_orbit_input(&config,0,1,0,0,1,2,0.01f,0.15f,0);
    close_to(config.pitch,1.3f);
    McFollowConfig frame30={0},frame60={0};
    for (unsigned i=0;i<30;++i)
        mc_orbit_input(&frame30,.6f,.2f,0,0,1.f/30,2,.01f,.15f,0);
    for (unsigned i=0;i<60;++i)
        mc_orbit_input(&frame60,.6f,.2f,0,0,1.f/60,2,.01f,.15f,0);
    close_to(frame30.yaw,frame60.yaw);close_to(frame30.pitch,frame60.pitch);
    McFollowConfig inverted={0};
    mc_orbit_input(&inverted,0,0,1,1,0,2,.01f,.15f,1);
    close_to(inverted.yaw,.01f);close_to(inverted.pitch,.01f);
    pose=(McPose){.position={0,0,600},.target={0,0,0}};
    float fraction=0.5f;
    assert(mc_resolve_collision(&pose,10,20,obstruction,&fraction)==1);
    close_to(pose.position.z,280);
    fraction=NAN;
    assert(mc_resolve_collision(&pose,10,20,obstruction,&fraction)==-1);
    close_to(pose.position.z,280);
    fraction=0;
    assert(mc_resolve_collision(&pose,10,20,obstruction,&fraction)==-1);
    close_to(pose.position.z,280);
    fraction=0.05f; /* Hit inside clearance: cannot form a safe look-at. */
    assert(mc_resolve_collision(&pose,10,20,obstruction,&fraction)==-1);
    close_to(pose.position.z,280);
    int queries=0;
    pose=(McPose){.position={0,0,600},.target={0,0,0}};
    assert(mc_resolve_collision_orbit(&pose,0,20,100,wall,&queries)==2);
    assert(queries<=6);
    assert(hypotf(pose.position.x,pose.position.z)>=100);
    assert(pose.position.z<=50);
    return 0;
}
