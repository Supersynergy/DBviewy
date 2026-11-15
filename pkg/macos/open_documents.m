#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>

extern void octa_menu_action(int tag);
extern void octa_open_document(const char *path);

@interface OctaDocuments : NSObject
@end

static OctaDocuments *documents;

@implementation OctaDocuments
- (void)performMenuAction:(NSMenuItem *)item { octa_menu_action((int)item.tag); }
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


// Called on the GUI thread once the winit application menu exists.
bool octa_add_workbook_menu(const char *const titles[]) {
    NSMenu *menu = NSApp.mainMenu.itemArray.firstObject.submenu;
    if (!menu || menu.numberOfItems < 1) return false;
    static NSMutableArray<NSMenuItem *> *items;
    if (items.count) {
        for (NSUInteger i = 0; i < items.count; ++i)
            items[i].title = [NSString stringWithUTF8String:titles[i]];
        return true;
    }
    items = [NSMutableArray array];
    NSArray *symbols = @[@"folder", @"rectangle.split.3x1", @"gearshape"];
    NSInteger index = 1;
    [menu insertItem:[NSMenuItem separatorItem] atIndex:index++];
    for (NSInteger i = 0; i < 3; ++i) {
        NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:[NSString stringWithUTF8String:titles[i]]
            action:@selector(performMenuAction:) keyEquivalent:@""];
        item.tag = i;
        item.target = documents;
        item.image = [NSImage imageWithSystemSymbolName:symbols[i] accessibilityDescription:nil];
        [items addObject:item];
        [menu insertItem:item atIndex:index++];
    }
    return true;
}
