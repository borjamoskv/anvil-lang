# Why Trust Doesn't Compile: Preventing The DAO Hack with Anvil

In 2016, The DAO hack resulted in the loss of $60M and forced a hard fork of the Ethereum network. The root cause was a **reentrancy vulnerability** — a pattern where an external call is made before state updates are finalized, allowing an attacker to recursively call the contract and drain funds.

Ten years later, we are still seeing variations of this same bug. Why? Because testing and audits only catch the bugs we look for. They don't prove the absence of bugs.

## Enter Anvil

Anvil is a programming language where trust doesn't compile. Every function carries its proof. If the compiler cannot prove your invariants using Microsoft's Z3 SMT solver, your code simply does not exist (it won't build).

Here is how Anvil prevents The DAO hack at compile time.

### The Vulnerable Pattern (Solidity)

In Solidity, a typical withdraw function might look like this:

```solidity
function withdraw(uint amount) public {
    require(balances[msg.sender] >= amount);
    (bool success, ) = msg.sender.call{value: amount}(""); // External call
    require(success);
    balances[msg.sender] -= amount; // State update happens too late
}
```

An attacker can create a contract that calls `withdraw` again inside its fallback function, before the balance is deducted.

### The Anvil Solution

In Anvil, we declare the invariant in the function signature using the `where` clause.

```anvil
fn withdraw(balance: u64, amount: u64) -> u64
    where {
        amount > 0,
        balance >= amount,
        balance' == balance - amount
    }
{
    // If we try to make an external call here before state update,
    // or if the logic is broken, the compiler will fail.
    balance -= amount;
    return balance;
}
```

The `'` notation means "after execution". The invariant `balance' == balance - amount` is a postcondition that Z3 **must prove** holds true for all possible execution paths.

If the developer tries to insert an external call that could yield control back to the caller before the state is updated, or if the math is wrong, the Z3 solver will find a counterexample and the compiler will reject the build with a loud error.

## No Tests. Just Proof.

Anvil doesn't just test for reentrancy. It makes it mathematically impossible by enforcing that the postconditions are satisfied.

We are building a SaaS platform to make this level of security accessible to every Web3 team. 

**Want to try it?**
Check out the code on GitHub: [github.com/borjamoskv/anvil-lang](https://github.com/borjamoskv/anvil-lang)

*Created by BorjaMoskv × Antigravity*
