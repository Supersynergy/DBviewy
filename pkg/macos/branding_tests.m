#import <AppKit/AppKit.h>
#import <assert.h>
void octa_menu_action(int tag) {}
void octa_open_document(const char *path) {}
extern void octa_set_identity(const char *version);
extern void octa_listen_for_documents(void);
extern bool octa_add_workbook_menu(const char *const titles[]);
int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        octa_set_identity("0.22.0-dbviewy.1");
        octa_listen_for_documents();
        NSMenu *root = [[NSMenu alloc] initWithTitle:@"Main"];
        NSMenuItem *app = [[NSMenuItem alloc] initWithTitle:@"octa" action:nil keyEquivalent:@""];
        NSMenu *menu = [[NSMenu alloc] initWithTitle:@"octa"];
        [menu addItemWithTitle:@"About octa" action:@selector(orderFrontStandardAboutPanel:) keyEquivalent:@""];
        [menu addItemWithTitle:@"Hide octa" action:@selector(hide:) keyEquivalent:@"h"];
        [menu addItemWithTitle:@"Hide Others" action:@selector(hideOtherApplications:) keyEquivalent:@"h"];
        [menu addItemWithTitle:@"Quit octa" action:@selector(terminate:) keyEquivalent:@"q"];
        app.submenu = menu; [root addItem:app]; NSApp.mainMenu=root;
        const char *titles[]={"Open...","Columns","Settings..."};
        assert(octa_add_workbook_menu(titles));
        assert(octa_add_workbook_menu(titles)); // no duplicates on locale refresh
        assert(menu.numberOfItems==8);
        assert([[menu itemAtIndex:0].title isEqualToString:@"About DBviewy"]);
        assert([menu itemAtIndex:0].target != nil);
        assert([menu itemAtIndex:0].action == NSSelectorFromString(@"showDBviewyAbout:"));
        assert([[menu itemAtIndex:5].title isEqualToString:@"Hide DBviewy"]);
        assert([[menu itemAtIndex:6].title isEqualToString:@"Hide Others"]);
        assert([[menu itemAtIndex:7].title isEqualToString:@"Quit DBviewy"]);
        assert([[menu itemAtIndex:7].keyEquivalent isEqualToString:@"q"]);
        puts("PASS: DBviewy native menu names, About target, shortcuts and idempotence");
    }
    return 0;
}
