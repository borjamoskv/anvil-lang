// DeXe Protocol - getSaleTokenAmount - Silicon Logic (Verilog)
// Purpose: Hardware-level deterministic verification of fixed-point arithmetic.

module getSaleTokenAmount (
    input [127:0] amount,
    input [127:0] exchange_rate,
    output [127:0] sale_token_amount
);
    // PRECISION = 10^25
    // sale_token_amount = (amount * 10^25) / exchange_rate

    wire [255:0] scaled_amount;
    assign scaled_amount = amount * 128'd10000000000000000000000000;
    
    assign sale_token_amount = scaled_amount / exchange_rate;

endmodule
