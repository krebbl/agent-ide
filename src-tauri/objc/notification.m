#import "notification.h"
#import <Foundation/Foundation.h>

static BOOL is_running_in_bundle(void) {
    NSString *bundlePath = [[NSBundle mainBundle] bundlePath];
    return [bundlePath hasSuffix:@".app"];
}

static void setup_ns_user_notification_delegate(void);

void show_notification(const char *title, const char *body) {
    if (!is_running_in_bundle()) {
        NSString *titleNs = [NSString stringWithUTF8String:title];
        NSString *bodyNs = [NSString stringWithUTF8String:body];
        NSLog(@"[dev] skipping native notification: %@ - %@", titleNs, bodyNs);
        return;
    }
    NSString *titleNs = [NSString stringWithUTF8String:title];
    NSString *bodyNs = [NSString stringWithUTF8String:body];
    dispatch_async(dispatch_get_main_queue(), ^{
        setup_ns_user_notification_delegate();
        NSUserNotification *notification = [[NSUserNotification alloc] init];
        notification.title = titleNs;
        notification.informativeText = bodyNs;
        notification.soundName = NSUserNotificationDefaultSoundName;
        [[NSUserNotificationCenter defaultUserNotificationCenter] deliverNotification:notification];
        NSLog(@"Delivered native notification: %@ - %@", titleNs, bodyNs);
    });
}

@interface AgentIDENotificationDelegate : NSObject <NSUserNotificationCenterDelegate>
@end

@implementation AgentIDENotificationDelegate

- (BOOL)userNotificationCenter:(NSUserNotificationCenter *)center shouldPresentNotification:(NSUserNotification *)notification {
    NSLog(@"shouldPresentNotification called");
    return YES;
}

- (void)userNotificationCenter:(NSUserNotificationCenter *)center didDeliverNotification:(NSUserNotification *)notification {
    NSLog(@"didDeliverNotification called");
}

- (void)userNotificationCenter:(NSUserNotificationCenter *)center didActivateNotification:(NSUserNotification *)notification {
    NSLog(@"didActivateNotification called");
}

@end

static AgentIDENotificationDelegate *g_notification_delegate = nil;

static void setup_ns_user_notification_delegate(void) {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        g_notification_delegate = [[AgentIDENotificationDelegate alloc] init];
        [NSUserNotificationCenter defaultUserNotificationCenter].delegate = g_notification_delegate;
        NSLog(@"NSUserNotificationCenter delegate set");
    });
}
