from decimal import Decimal

from loans.emi import emi


def test_emi_standard():
    assert emi(Decimal("100000"), Decimal("12"), 12) == Decimal("8884.88")


def test_emi_zero_rate():
    assert emi(Decimal("1200"), Decimal("0"), 12) == Decimal("100.00")
