#import "webview_gestures.h"
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

static WKWebView *find_webview(NSView *view) {
  if ([view isKindOfClass:[WKWebView class]]) {
    return (WKWebView *)view;
  }
  for (NSView *subview in view.subviews) {
    WKWebView *found = find_webview(subview);
    if (found) {
      return found;
    }
  }
  return nil;
}

void enable_webview_back_forward_gestures(void *ns_view) {
  if (!ns_view) {
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    WKWebView *webview = find_webview((__bridge NSView *)ns_view);
    if (webview) {
      webview.allowsBackForwardNavigationGestures = YES;
    } else {
      NSLog(@"[agent-ide] WKWebView not found for back/forward gestures");
    }
  });
}
