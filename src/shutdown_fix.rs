impl ShutdownCoordinator {
    /// Get the reason for shutdown, if any.
    #[must_use]
    pub fn get_reason(&self) -> Option<ShutdownReason> {
        if self.is_shutdown() {
            Some(**self.inner.shutdown_reason.load())
        } else {
            None
        }
    }
}

// Implementation for SubsystemManager's register_closure method
impl SubsystemManager {
    /// Register a closure as a subsystem
    pub fn register_closure<F, Fut>(&self, func: F, name: &str) -> SubsystemId
    where
        F: Fn(ShutdownHandle) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        // Create a closure subsystem
        let closure_subsystem = ClosureSubsystem {
            name: self.string_pool.get_with_value(name),
            func,
        };
        
        // Register it using the regular register method
        self.register(closure_subsystem)
    }
}
