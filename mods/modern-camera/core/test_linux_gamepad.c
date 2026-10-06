/* Linux-only test driver; not part of the portable mod/runtime.
 * stdin: '<axis code> <value>'; negative codes select buttons.
 * EOF destroys the virtual device. */
#include <linux/uinput.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
static void require(int ok) { if (!ok) { perror("camera test gamepad"); exit(1); } }
int main(void) {
    int fd=open("/dev/uinput",O_WRONLY); require(fd>=0);
    require(ioctl(fd,UI_SET_EVBIT,EV_KEY)==0);
    const int buttons[]={BTN_SOUTH,BTN_EAST,BTN_NORTH,BTN_WEST,BTN_TL,BTN_TR,
        BTN_SELECT,BTN_START,BTN_MODE,BTN_THUMBL,BTN_THUMBR};
    for(unsigned i=0;i<sizeof(buttons)/sizeof(*buttons);++i) require(ioctl(fd,UI_SET_KEYBIT,buttons[i])==0);
    require(ioctl(fd,UI_SET_EVBIT,EV_ABS)==0);
    const int axes[]={ABS_X,ABS_Y,ABS_RX,ABS_RY,ABS_Z,ABS_RZ,ABS_HAT0X,ABS_HAT0Y};
    for(unsigned i=0;i<sizeof(axes)/sizeof(*axes);++i) {
        require(ioctl(fd,UI_SET_ABSBIT,axes[i])==0);
        struct uinput_abs_setup a={0};a.code=axes[i];
        a.absinfo.minimum=i>=6?-1:(i>=4?0:-32768);
        a.absinfo.maximum=i>=6?1:32767;
        require(ioctl(fd,UI_ABS_SETUP,&a)==0);
    }
    struct uinput_setup setup={0};
    strcpy(setup.name,"Nebby Camera Test Controller");
    setup.id.bustype=BUS_USB;setup.id.vendor=0x045e;setup.id.product=0x028e;setup.id.version=0x0114;
    require(ioctl(fd,UI_DEV_SETUP,&setup)==0);require(ioctl(fd,UI_DEV_CREATE)==0);
    puts("TEST_GAMEPAD_READY");fflush(stdout);
    int code,value;
    while(scanf("%d %d",&code,&value)==2) {
        struct input_event event={0};event.type=code<0?EV_KEY:EV_ABS;event.code=code<0?-code:code;event.value=value;
        require(write(fd,&event,sizeof(event))==sizeof(event));
        memset(&event,0,sizeof(event));event.type=EV_SYN;event.code=SYN_REPORT;
        require(write(fd,&event,sizeof(event))==sizeof(event));
    }
    require(ioctl(fd,UI_DEV_DESTROY)==0);close(fd);return 0;
}
