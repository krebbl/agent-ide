#import "notification.h"
#import <Foundation/Foundation.h>

// Implemented in Rust; called when the user clicks the notification.
extern void agent_ide_notification_clicked(const char *session_id);

static BOOL is_running_in_bundle(void) {
    NSString *bundlePath = [[NSBundle mainBundle] bundlePath];
    return [bundlePath hasSuffix:@".app"];
}

static void setup_ns_user_notification_delegate(void);

void show_notification(const char *title, const char *body, const char *session_id) {
    if (!is_running_in_bundle()) {
        NSString *titleNs = [NSString stringWithUTF8String:title];
        NSString *bodyNs = [NSString stringWithUTF8String:body];
        NSLog(@"[dev] skipping native notification: %@ - %@", titleNs, bodyNs);
        return;
    }
    NSString *titleNs = [NSString stringWithUTF8String:title];
    NSString *bodyNs = [NSString stringWithUTF8String:body];
    NSString *sessionIdNs = session_id ? [NSString stringWithUTF8String:session_id] : nil;
    dispatch_async(dispatch_get_main_queue(), ^{
        setup_ns_user_notification_delegate();
        NSUserNotification *notification = [[NSUserNotification alloc] init];
        notification.title = titleNs;
        notification.informativeText = bodyNs;
        notification.soundName = NSUserNotificationDefaultSoundName;
        if (sessionIdNs) {
            notification.identifier = sessionIdNs;
        }
        [[NSUserNotificationCenter defaultUserNotificationCenter] deliverNotification:notification];
        NSLog(@"Delivered native notification: %@ - %@ (%@)", titleNs, bodyNs, sessionIdNs ?: @"");
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
    if (notification.identifier) {
        agent_ide_notification_clicked([notification.identifier UTF8String]);
    }
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
