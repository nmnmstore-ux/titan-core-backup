pragma circom 2.1.6;

include "circomlib/circuits/comparators.circom";
include "circomlib/circuits/bitify.circom";
include "circomlib/circuits/poseidon.circom";

template ClearingPriceProof(MAX_ORDERS) {
    signal input buy_prices[MAX_ORDERS];
    signal input buy_quantities[MAX_ORDERS];
    signal input sell_prices[MAX_ORDERS];
    signal input sell_quantities[MAX_ORDERS];
    signal input buy_count;
    signal input sell_count;
    signal input clearing_price;

    signal output valid;

    component sorted_buys = SortDesc(MAX_ORDERS);
    component sorted_sells = SortAsc(MAX_ORDERS);

    for (var i = 0; i < MAX_ORDERS; i++) {
        sorted_buys.in[i] <== buy_prices[i];
        sorted_sells.in[i] <== sell_prices[i];
    }

    signal max_match_price;
    max_match_price <== 0;

    var match_found = 0;

    for (var b = 0; b < MAX_ORDERS; b++) {
        var buy_idx = b;
        if (buy_idx < buy_count) {
            var buy_price = sorted_buys.out[buy_idx];
            var buy_qty = buy_quantities[buy_idx];

            for (var s = 0; s < MAX_ORDERS; s++) {
                var sell_idx = s;
                if (sell_idx < sell_count) {
                    var sell_price = sorted_sells.out[sell_idx];
                    var sell_qty = sell_quantities[sell_idx];

                    var can_match = buy_price >= sell_price;
                    var is_first_match = (match_found == 0) * can_match;

                    max_match_price = max_match_price + is_first_match * ((buy_price + sell_price) / 2);
                    match_found = match_found + is_first_match;
                }
            }
        }
    }

    var price_correct = (clearing_price == max_match_price);
    valid <== price_correct;
}

template SortAsc(n) {
    signal input in[n];
    signal output out[n];

    component sort = MergeSortAsc(n);
    for (var i = 0; i < n; i++) {
        sort.in[i] <== in[i];
        out[i] <== sort.out[i];
    }
}

template SortDesc(n) {
    signal input in[n];
    signal output out[n];

    component sort = MergeSortDesc(n);
    for (var i = 0; i < n; i++) {
        sort.in[i] <== in[i];
        out[i] <== sort.out[i];
    }
}

template MergeSortAsc(n) {
    signal input in[n];
    signal output out[n];

    if (n == 1) {
        out[0] <== in[0];
    } else {
        var half = n / 2;
        signal left[half];
        signal right[n - half];

        for (var i = 0; i < half; i++) {
            left[i] <== in[i];
        }
        for (var i = half; i < n; i++) {
            right[i - half] <== in[i];
        }

        component left_sort = MergeSortAsc(half);
        component right_sort = MergeSortAsc(n - half);

        for (var i = 0; i < half; i++) {
            left_sort.in[i] <== left[i];
        }
        for (var i = 0; i < n - half; i++) {
            right_sort.in[i] <== right[i];
        }

        component merge = MergeAsc(half, n - half);
        for (var i = 0; i < half; i++) {
            merge.left[i] <== left_sort.out[i];
        }
        for (var i = 0; i < n - half; i++) {
            merge.right[i] <== right_sort.out[i];
        }

        for (var i = 0; i < n; i++) {
            out[i] <== merge.out[i];
        }
    }
}

template MergeSortDesc(n) {
    signal input in[n];
    signal output out[n];

    if (n == 1) {
        out[0] <== in[0];
    } else {
        var half = n / 2;
        signal left[half];
        signal right[n - half];

        for (var i = 0; i < half; i++) {
            left[i] <== in[i];
        }
        for (var i = half; i < n; i++) {
            right[i - half] <== in[i];
        }

        component left_sort = MergeSortDesc(half);
        component right_sort = MergeSortDesc(n - half);

        for (var i = 0; i < half; i++) {
            left_sort.in[i] <== left[i];
        }
        for (var i = 0; i < n - half; i++) {
            right_sort.in[i] <== right[i];
        }

        component merge = MergeDesc(half, n - half);
        for (var i = 0; i < half; i++) {
            merge.left[i] <== left_sort.out[i];
        }
        for (var i = 0; i < n - half; i++) {
            merge.right[i] <== right_sort.out[i];
        }

        for (var i = 0; i < n; i++) {
            out[i] <== merge.out[i];
        }
    }
}

template MergeAsc(n_left, n_right) {
    signal input left[n_left];
    signal input right[n_right];
    signal output out[n_left + n_right];

    component cmp[n_left + n_right];
    var i = 0, j = 0, k = 0;

    signal merged[n_left + n_right];
    var total = n_left + n_right;

    for (var x = 0; x < total; x++) {
        var take_left = (i < n_left) && (j >= n_right || left[i] <= right[j]);
        merged[x] <== take_left * left[i] + (1 - take_left) * right[j];
        i = i + take_left;
        j = j + (1 - take_left);
    }

    for (var x = 0; x < total; x++) {
        out[x] <== merged[x];
    }
}

template MergeDesc(n_left, n_right) {
    signal input left[n_left];
    signal input right[n_right];
    signal output out[n_left + n_right];

    var total = n_left + n_right;
    signal merged[total];
    var i = 0, j = 0, k = 0;

    for (var x = 0; x < total; x++) {
        var take_left = (i < n_left) && (j >= n_right || left[i] >= right[j]);
        merged[x] <== take_left * left[i] + (1 - take_left) * right[j];
        i = i + take_left;
        j = j + (1 - take_left);
    }

    for (var x = 0; x < total; x++) {
        out[x] <== merged[x];
    }
}

component main = ClearingPriceProof(64);