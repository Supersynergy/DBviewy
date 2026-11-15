#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>

extern void octa_open_document(const char *path);

@interface OctaDocuments : NSObject
@end

@implementation OctaDocuments
- (void)openDocuments:(NSAppleEventDescriptor *)event reply:(NSAppleEventDescriptor *)reply {
    NSAppleEventDescriptor *documents = [event paramDescriptorForKeyword:keyDirectObject];
    for (NSInteger i = 1; i <= documents.numberOfItems; ++i) {
        NSURL *url = [documents descriptorAtIndex:i].fileURLValue;
        if (url.isFileURL && url.fileSystemRepresentation)
            octa_open_document(url.fileSystemRepresentation);
    }
}
@end

void octa_listen_for_documents(void) {
    static OctaDocuments *documents;
    documents = [[OctaDocuments alloc] init];
    // Winit owns the NSApplication delegate. Install only the open-documents
    // event after AppKit initialises its handlers and before it drains events.
    [NSNotificationCenter.defaultCenter
        addObserverForName:NSApplicationWillFinishLaunchingNotification
        object:nil queue:nil usingBlock:^(NSNotification *note) {
            [NSAppleEventManager.sharedAppleEventManager
                setEventHandler:documents andSelector:@selector(openDocuments:reply:)
                forEventClass:kCoreEventClass andEventID:kAEOpenDocuments];
        }];
}
