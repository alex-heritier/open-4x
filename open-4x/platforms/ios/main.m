// Winit owns UIApplication and the native event loop.
extern void fourx_ios_main(void);
int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    fourx_ios_main();
    return 0;
}
