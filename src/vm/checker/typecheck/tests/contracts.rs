use vm::parser::parse;
use vm::checker::{AnalysisDatabase,identity_pass};

#[test]
fn test_names_tokens_contracts() {
    use vm::checker::type_check;
    let tokens_contract = 
        "(define-map tokens ((account principal)) ((balance int)))
         (define (get-balance (account principal))
            (let ((balance
                  (get balance (fetch-entry tokens (tuple (account account))))))
              (if (eq? balance 'null) 0 balance)))

         (define (token-credit! (account principal) (tokens int))
            (if (<= tokens 0)
                'false
                (let ((current-amount (get-balance account)))
                  (begin
                    (set-entry! tokens (tuple (account account))
                                       (tuple (balance (+ tokens current-amount))))
                    'true))))
         (define-public (token-transfer (to principal) (amount int))
          (let ((balance (get-balance tx-sender)))
             (if (or (> amount balance) (<= amount 0))
                 'false
                 (begin
                   (set-entry! tokens (tuple (account tx-sender))
                                      (tuple (balance (- balance amount))))
                   (token-credit! to amount)))))                     
         (begin (token-credit! 'SZ2J6ZY48GV1EZ5V2V5RB9MP66SW86PYKKQ9H6DPR 10000)
                (token-credit! 'SM2J6ZY48GV1EZ5V2V5RB9MP66SW86PYKKQVX8X0G 300)
                'null)";

    let names_contract =
        "(define burn-address 'SP000000000000000000002Q6VF78)
         (define (price-function (name int))
           (if (< name 100000) 1000 100))
         
         (define-map name-map 
           ((name int)) ((owner principal)))
         (define-map preorder-map
           ((name-hash (buff 20)))
           ((buyer principal) (paid int)))
         
         (define-public (preorder 
                        (name-hash (buff 20))
                        (name-price int))
           (if (contract-call! tokens token-transfer
                 burn-address name-price)
               (insert-entry! preorder-map
                 (tuple (name-hash name-hash))
                 (tuple (paid name-price)
                        (buyer tx-sender)))
               'false))

         (define-public (register 
                        (recipient-principal principal)
                        (name int)
                        (salt int))
           (let ((preorder-entry
                   (fetch-entry preorder-map
                                  (tuple (name-hash (hash160 (xor name salt))))))
                 (name-entry 
                   (fetch-entry name-map (tuple (name name)))))
             (if (and
                  ;; must be preordered
                  (not (eq? preorder-entry 'null))
                  ;; name shouldn't *already* exist
                  (eq? name-entry 'null)
                  ;; preorder must have paid enough
                  (<= (price-function name) 
                      (get paid preorder-entry))
                  ;; preorder must have been the current principal
                  (eq? tx-sender
                       (get buyer preorder-entry)))
                  (and
                    (insert-entry! name-map
                      (tuple (name name))
                      (tuple (owner recipient-principal)))
                    (delete-entry! preorder-map
                      (tuple (name-hash (hash160 (xor name salt))))))
                  'false)))";

    let mut tokens_contract = parse(tokens_contract).unwrap();
    let mut names_contract = parse(names_contract).unwrap();
    let mut db = AnalysisDatabase::memory();

    type_check(&"tokens", &mut tokens_contract, &mut db).unwrap();
    type_check(&"names", &mut names_contract, &mut db).unwrap();
}