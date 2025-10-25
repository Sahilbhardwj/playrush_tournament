use anchor_lang::prelude::*;

declare_id!("DDg5ztC3T5K8meTgBZMQX1oqjCGmFJgEgVrXUTjhLFNP");



#[program]
pub mod playrush_tournament {
    use super::*;

    pub fn initialize_tournament(ctx: Context<InitializeTournament>, entry_fee: u64) -> Result<()> {
        msg!("Tournament created with entry fee: {}", entry_fee);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeTournament<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
}

