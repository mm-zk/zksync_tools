import sys
from prover import prove_tx_inclusion_in_chain

def main():
    # Check if at least one argument is provided
    if len(sys.argv) > 1:
        transaction_id = sys.argv[1]
        if len(transaction_id) != 66 or transaction_id[:2] != "0x":
            print("Please pass correct transaction id. For example 0xb07cf51bb1fb788e9ab4961af203ce1057cf40f2781007ff06e7c66b6fc814be")    
            return
        prove_tx_inclusion_in_chain(transaction_id)
        
    else:
        print("Please pass transaction id. For example 0xb07cf51bb1fb788e9ab4961af203ce1057cf40f2781007ff06e7c66b6fc814be")

if __name__ == "__main__":
    main()