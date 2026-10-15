# Handle outages and protect access

An unavailable website or relayer does not transfer ownership of your funds.
Your wallet remains the authority for funded withdrawals, PENCIL minting, staking actions, and reward claims.
An independent Solana client can submit those actions directly without a fresh oracle price or the relayer's trading signature.

Available funded USDC remains distinct from open margin, reserved cash, and unpaid profit claims.
An outage does not turn an unpaid claim into withdrawable cash.

## Close during a relayer outage

An owner can submit an onchain fallback exit request for a position.
After 15 seconds, any caller can enqueue the requested close with a fresh checkpoint.
The close still follows the execution queue, price rules, and any owner-specified price limit.
It cannot guarantee a fill at the price shown when the outage began.

The website's normal order flow uses the relayer. A direct fallback action requires a compatible independent client.
Avoid granting unrestricted wallet authority to an unknown recovery service.

## Understand relayer removal

After Seal, 24 hours without included relayer admission or heartbeat permits anyone to submit an explicit relayer-removal transaction.
That transaction enables direct owner or delegate admission and removes guardian halt power.
The timeout does not trigger removal automatically. A later heartbeat cannot restore a role after its removal.

Owner authorization, delegate scope, margin reservations, expiry, oracle rules, and FIFO execution still apply after removal.
Removing the relayer does not forgive debts or guarantee profit payments.

## Understand a permanently stopped market

For a sealed market, seven days without an advancing accepted checkpoint can permit permanent market wind-down.
At least 756,000 slots must also elapse. A slot is a Solana block opportunity.
Before wind-down, a valid newer checkpoint can preserve a healthy market.

Wind-down freezes the market's last accepted mark. Anyone can settle its positions against that frozen price without a current oracle observation.
The frozen price can differ from current external prices. The market cannot restart after a permanent halt.
If every market stops, new deposits also stop.

Cancelled orders release their reserved cash or restore their original claim reservation once.
Cash-backed principal and available cash remain withdrawable after settlement.
Unfunded profits and claim-funded principal remain unsecured claims until the house can repay them.
Stopped-market settlement takes no trading fee and allocates no LP profit share.

These recovery paths cannot create missing USDC, guarantee full repayment, or repair an arbitrary contract defect.
They depend on Solana remaining available and the relevant instructions continuing to function.
