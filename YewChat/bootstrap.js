import('./pkg').then((module) => {
    module.default().then(() => {
        module.run_app();
    });
});
