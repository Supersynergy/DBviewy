#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>

extern void octa_menu_action(int tag);
extern void octa_open_document(const char *path);

@interface OctaDocuments : NSObject
@end

static OctaDocuments *documents;
static NSString *dbviewyVersion;

void octa_set_identity(const char *version) {
    dbviewyVersion = [NSString stringWithUTF8String:version];
}

@implementation OctaDocuments
- (void)performMenuAction:(NSMenuItem *)item { octa_menu_action((int)item.tag); }
- (void)showDBviewyAbout:(id)sender {
    NSBundle *bundle = NSBundle.mainBundle;
    // Load by URL instead of AppKit's named-image cache: an update may have
    // replaced the icon while the old application was still running.
    NSString *name = [bundle objectForInfoDictionaryKey:@"CFBundleIconFile"] ?: @"dbviewy";
    NSURL *url = [bundle URLForResource:name withExtension:@"icns"];
    NSImage *icon = url ? [[NSImage alloc] initWithContentsOfURL:url] : nil;
    NSMutableDictionary *options = [@{
        NSAboutPanelOptionApplicationName: @"DBviewy",
        NSAboutPanelOptionApplicationVersion: dbviewyVersion ?: @"",
        NSAboutPanelOptionVersion: [bundle objectForInfoDictionaryKey:@"CFBundleVersion"] ?: @""
    } mutableCopy];
    if (icon) options[NSAboutPanelOptionApplicationIcon] = icon;
    [NSApp orderFrontStandardAboutPanelWithOptions:options];
}

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
    for (NSMenuItem *item in menu.itemArray) {
        if (item.action == @selector(orderFrontStandardAboutPanel:) ||
            item.action == @selector(showDBviewyAbout:) ||
            item.action == @selector(hide:) || item.action == @selector(terminate:)) {
            item.title = [item.title stringByReplacingOccurrencesOfString:@"octa"
                withString:@"DBviewy" options:NSCaseInsensitiveSearch
                range:NSMakeRange(0, item.title.length)];
        }
        if (item.action == @selector(orderFrontStandardAboutPanel:)) {
            item.action = @selector(showDBviewyAbout:);
            item.target = documents;
        }
    }

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
