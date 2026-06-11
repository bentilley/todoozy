use super::IDStrategy;

pub struct GitIDStrategy {
    // repo: git2::Repository,
    // remote_name: String,
}

impl GitIDStrategy {
    fn fetch_id_tags(&self, remote_name: &str) -> Result<()> {
        let mut remote = self
            .repo
            .find_remote(remote_name)
            .map_err(|_| Error::Custom(format!("no remote '{remote_name}' configured")))?;
        let mut opts = git2::FetchOptions::new();
        opts.remote_callbacks(Self::make_credentials_callback());
        opts.download_tags(git2::AutotagOption::None);
        // Ignore errors: remote may have no tdz/* tags yet
        let _ = remote.fetch(&["refs/tags/tdz/*:refs/tags/tdz/*"], Some(&mut opts), None);
        Ok(())
    }

}

impl IDStrategy for GitIDStrategy {
    fn next(&mut self) -> super::error::Result<u32> {
        self.fetch_id_tags(remote_name)?;
        let start = self.max_local_tag_id() + 1;
        for id in start.. {
            if self.try_push_tag(remote_name, id)? {
                return Ok(id);
            }
        }
        unreachable!()
    }
}
