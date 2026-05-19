import('./pkg/yewchat.js').then((module) => {
    module.default().then(() => {
        module.run_app();
    });
});
