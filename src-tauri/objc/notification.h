#ifndef AGENT_IDE_NOTIFICATION_H
#define AGENT_IDE_NOTIFICATION_H

#ifdef __cplusplus
extern "C" {
#endif

void request_notification_permission(void);
void show_notification(const char *title, const char *body, const char *session_id);

#ifdef __cplusplus
}
#endif

#endif
