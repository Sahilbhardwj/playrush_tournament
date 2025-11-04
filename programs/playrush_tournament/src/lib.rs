use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use std::str::FromStr;

declare_id!("4SAtdRVoDTBvPSv56qNVVnY3v6HaeZYTFBhyUsTaPZG8");

#[program]
pub mod playrush_tournament {
    use super::*;

    pub fn initialize_tournament(
        ctx: Context<InitializeTournament>,
        game_id: String,
        tournament_id: String,
        is_token: bool, // false => SOL, true => PR Token
    ) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        tournament.authority = ctx.accounts.treasury.key();
        tournament.game_id = game_id;
        tournament.tournament_id = tournament_id;
        tournament.is_token = is_token;
        tournament.total_pool = 0;
        tournament.is_active = true;
        tournament.bump = ctx.bumps.tournament;
        Ok(())
    }

    // Join Tournament with SOL
    pub fn join_tournament_sol(ctx: Context<JoinTournamentSol>, amount: u64) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        require!(tournament.is_active, PlayrushError::TournamentClosed);
        require!(!tournament.is_token, PlayrushError::InvalidJoinMethod);

        // Split 10% treasury, 90% pool
        let treasury_cut = amount / 10;
        let pool_share = amount - treasury_cut;

        // Transfer SOL to Treasury
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.player.key(),
                &ctx.accounts.treasury.key(),
                treasury_cut,
            ),
            &[
                ctx.accounts.player.to_account_info(),
                ctx.accounts.treasury.to_account_info(),
            ],
        )?;

        // Transfer SOL to Tournament PDA
        let tournament_key = tournament.key(); // extract key before mutable borrow ends
        let tournament_info = tournament.to_account_info(); // same here

        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.player.key(),
                &tournament_key,
                pool_share,
            ),
            &[ctx.accounts.player.to_account_info(), tournament_info],
        )?;

        // Now safe to mutate again
        tournament.total_pool = tournament
            .total_pool
            .checked_add(pool_share)
            .ok_or(PlayrushError::Overflow)?;

        // Player entry PDA initialization
        let entry = &mut ctx.accounts.player_entry;
        entry.tournament = tournament_key;
        entry.player = ctx.accounts.player.key();
        entry.joined_at = Clock::get()?.unix_timestamp;
        entry.score = 0;
        entry.bump = ctx.bumps.player_entry;

        msg!(
            "Player {} joined tournament {} with {} lamports",
            entry.player,
            tournament_key,
            amount
        );

        Ok(())
    }

    // Join Tournament with PR Token
    pub fn join_tournament_token(ctx: Context<JoinTournamentToken>, amount: u64) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        require!(tournament.is_active, PlayrushError::TournamentClosed);
        require!(tournament.is_token, PlayrushError::InvalidJoinMethod);
        let expected_mint =
            Pubkey::from_str("CKxGC6cYjhzSq5c7dNKGjLRrhqw9YqbLbUn652qM2h1b").unwrap();
        require!(
            ctx.accounts.player_token_account.mint == expected_mint,
            PlayrushError::InvalidTokenMint
        );
        require!(
            ctx.accounts.pool_token_account.mint == expected_mint,
            PlayrushError::InvalidTokenMint
        );
        require!(
            ctx.accounts.treasury_token_account.mint == expected_mint,
            PlayrushError::InvalidTokenMint
        );
        // Split 10% treasury, 90% pool
        let treasury_cut = amount / 10;
        let pool_share = amount - treasury_cut;

        // Transfer 10% to Treasury
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.player_token_account.to_account_info(),
                    to: ctx.accounts.treasury_token_account.to_account_info(),
                    authority: ctx.accounts.player.to_account_info(),
                },
            ),
            treasury_cut,
        )?;

        // Transfer 90% to Pool
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.player_token_account.to_account_info(),
                    to: ctx.accounts.pool_token_account.to_account_info(),
                    authority: ctx.accounts.player.to_account_info(),
                },
            ),
            pool_share,
        )?;

        tournament.total_pool = tournament
            .total_pool
            .checked_add(pool_share)
            .ok_or(PlayrushError::Overflow)?;

        Ok(())
    }

    // Close Tournament Entries
    pub fn close_entry(ctx: Context<CloseEntry>) -> Result<()> {
        let tournament = &mut ctx.accounts.tournament;
        tournament.is_active = false;
        Ok(())
    }
    //free to earn  rewards distribution
    pub fn distribute_rewards(ctx: Context<DistributeRewards>) -> Result<()> {
        let tournament = &ctx.accounts.tournament;
        let total = tournament.total_pool;
        let first_amt = total * 50 / 100;
        let second_amt = total * 30 / 100;
        let third_amt = total * 20 / 100;

        let seeds = &[
            b"playrush",
            tournament.game_id.as_bytes(),
            tournament.tournament_id.as_bytes(),
            &[tournament.bump],
        ];
        let signer: &[&[&[u8]]] = &[&seeds[..]];

        if tournament.is_token {
            // --- Token reward path ---
            let token_program = ctx.accounts.token_program.to_account_info();
            let pool_token_account = ctx.accounts.pool_token_account.to_account_info();
            let tournament_ai = tournament.to_account_info();

            // 1st place
            token::transfer(
                CpiContext::new_with_signer(
                    token_program.clone(),
                    Transfer {
                        from: pool_token_account.clone(),
                        to: ctx.accounts.first_place_account.to_account_info(),
                        authority: tournament_ai.clone(),
                    },
                    signer,
                ),
                first_amt,
            )?;

            // 2nd place
            token::transfer(
                CpiContext::new_with_signer(
                    token_program.clone(),
                    Transfer {
                        from: pool_token_account.clone(),
                        to: ctx.accounts.second_place_account.to_account_info(),
                        authority: tournament_ai.clone(),
                    },
                    signer,
                ),
                second_amt,
            )?;

            // 3rd place
            token::transfer(
                CpiContext::new_with_signer(
                    token_program,
                    Transfer {
                        from: pool_token_account,
                        to: ctx.accounts.third_place_account.to_account_info(),
                        authority: tournament_ai,
                    },
                    signer,
                ),
                third_amt,
            )?;
        } else {
            // --- SOL reward path ---
            let tournament_ai = tournament.to_account_info();
            let system_program = ctx.accounts.system_program.to_account_info();

            for (dest, amount) in [
                (ctx.accounts.first_place.key(), first_amt),
                (ctx.accounts.second_place.key(), second_amt),
                (ctx.accounts.third_place.key(), third_amt),
            ] {
                let ix = anchor_lang::solana_program::system_instruction::transfer(
                    &tournament.key(),
                    &dest,
                    amount,
                );

                anchor_lang::solana_program::program::invoke_signed(
                    &ix,
                    &[tournament_ai.clone(), system_program.clone()],
                    signer,
                )?;
            }
        }

        Ok(())
    }

    //test  instruction
    pub fn test_idl(_ctx: Context<TestIdl>) -> Result<()> {
        msg!(" PlayRush Tournament Program connection successful!");
        Ok(())
    }
pub fn distribute_free2earn_rewards(ctx: Context<DistributeFree2EarnRewards>) -> Result<()> {
    // clone AccountInfo before mut borrow
    let pool_info = ctx.accounts.pool.to_account_info();
    let treasury_info = ctx.accounts.treasury_token_account.to_account_info();
    let token_program = ctx.accounts.token_program.to_account_info();

    // mutable borrow now safe
    let pool = &mut ctx.accounts.pool;
    let treasury_balance = ctx.accounts.treasury_token_account.amount;

    // Calculate inflow
    let inflow = treasury_balance
        .checked_sub(pool.last_recorded_treasury)
        .ok_or(PlayrushError::Overflow)?;

    if inflow == 0 {
        msg!("No new inflows to distribute this month.");
        return Ok(());
    }

    // 10% allocation
    let allocation = inflow / 10;

    // Reward splits
    let first = allocation * 25 / 100;
    let second = allocation * 15 / 100;
    let third = allocation * 10 / 100;
    let rest_total = allocation * 50 / 100;
    let rest_share = rest_total / 7;

    let rewards = [
        first, second, third,
        rest_share, rest_share, rest_share, rest_share, rest_share, rest_share, rest_share,
    ];

    let signer_seeds: &[&[u8]] = &[b"free2earn", &[pool.bump]];

    let recipients = [
        &ctx.accounts.first_place_account,
        &ctx.accounts.second_place_account,
        &ctx.accounts.third_place_account,
        &ctx.accounts.fourth_place_account,
        &ctx.accounts.fifth_place_account,
        &ctx.accounts.sixth_place_account,
        &ctx.accounts.seventh_place_account,
        &ctx.accounts.eighth_place_account,
        &ctx.accounts.ninth_place_account,
        &ctx.accounts.tenth_place_account,
    ];

    for (i, recipient) in recipients.iter().enumerate() {
        token::transfer(
            CpiContext::new_with_signer(
                token_program.clone(),
                Transfer {
                    from: treasury_info.clone(),
                    to: recipient.to_account_info(),
                    authority: pool_info.clone(),
                },
                &[signer_seeds],
            ),
            rewards[i],
        )?;
    }

    // Update pool data after all transfers
    pool.last_recorded_treasury = treasury_balance;
    pool.total_distributed = pool
        .total_distributed
        .checked_add(allocation)
        .ok_or(PlayrushError::Overflow)?;

    msg!(
        "Distributed {} tokens as Free2Earn rewards (10% of inflow)",
        allocation
    );

    Ok(())
}


}

//
// ─── ACCOUNT STRUCTS ─────────────────────────────────────────────────────────────
//

#[derive(Accounts)]
#[instruction(game_id: String, tournament_id: String)]
pub struct InitializeTournament<'info> {
    #[account(
        init,
        payer = treasury,
        space = 8 + 32 + 64 + 64 + 1 + 1 + 8 + 8,
        seeds = [b"playrush", game_id.as_bytes(), tournament_id.as_bytes()],
        bump
    )]
    pub tournament: Account<'info, Tournament>,

    #[account(mut)]
    pub treasury: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct JoinTournamentSol<'info> {
    #[account(mut)]
    pub tournament: Account<'info, Tournament>,

    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        init,
        payer = player,
        space = 8 + 32 + 32 + 8 + 8 + 1,
        seeds = [b"player", player.key().as_ref(), tournament.key().as_ref()],
        bump
    )]
    pub player_entry: Account<'info, PlayerEntry>,

    /// Treasury receives 10% of SOL
    #[account(mut)]
    pub treasury: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct JoinTournamentToken<'info> {
    #[account(mut)]
    pub tournament: Account<'info, Tournament>,
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseEntry<'info> {
    #[account(mut, has_one = authority)]
    pub tournament: Account<'info, Tournament>,
    pub authority: Signer<'info>,
}
#[derive(Accounts)]
pub struct DistributeRewards<'info> {
    #[account(mut)]
    pub tournament: Account<'info, Tournament>,

    #[account(mut)]
    pub first_place: UncheckedAccount<'info>,

   
    #[account(mut)]
    pub second_place: UncheckedAccount<'info>,

    #[account(mut)]
    pub third_place: UncheckedAccount<'info>,

    // Token mode accounts
    #[account(mut)]
    pub pool_token_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub first_place_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub second_place_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub third_place_account: UncheckedAccount<'info>,

    pub token_program: Program<'info, token::Token>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct TestIdl {}

#[derive(Accounts)]
pub struct DistributeFree2EarnRewards<'info> {
    #[account(
        mut,
        seeds = [b"free2earn"],
        bump = pool.bump
    )]
    pub pool: Account<'info, Free2EarnPool>,

    #[account(mut)]
    pub treasury_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub first_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub second_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub third_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub fourth_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub fifth_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub sixth_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub seventh_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub eighth_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub ninth_place_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub tenth_place_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// ─── STATE STRUCTS ───────────────────────────────────────────────────────────────
//

#[account]
pub struct Tournament {
    pub authority: Pubkey,     // Treasury owner
    pub game_id: String,       // Game identifier
    pub tournament_id: String, // Tournament ID
    pub is_token: bool,        // true = PR Token, false = SOL
    pub total_pool: u64,
    pub is_active: bool,
    pub bump: u8,
}

#[account]
pub struct PlayerEntry {
    pub tournament: Pubkey,
    pub player: Pubkey,
    pub joined_at: i64,
    pub score: u64,
    pub bump: u8,
}

#[account]
pub struct Free2EarnPool {
    pub last_recorded_treasury: u64, // balance snapshot for previous month
    pub total_distributed: u64,      // total lifetime distributed
    pub bump: u8,
}

// ─── ERRORS ─────────────────────────────────────────────────────────────────────
//

#[error_code]
pub enum PlayrushError {
    #[msg("Tournament entries are closed")]
    TournamentClosed,
    #[msg("Wrong join method for this tournament")]
    InvalidJoinMethod,
    #[msg("Invalid destination account for payout")]
    InvalidDestination,
    #[msg("Overflow occurred during math operation")]
    Overflow,
    #[msg("Token mint is not the PR token")]
    InvalidTokenMint,
}
